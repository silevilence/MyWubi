//! Offline rendering measurements and acknowledged, multi-process skin reload stress.
//! No keyboard injection, window manipulation, or claim of compositor presentation time.
use core_engine::{
    skin::{
        preferences::{Mode, Selection},
        runtime,
    },
    Config,
};
use serde_json::{json, Value};
use std::{
    fs,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};
use text_render::{
    candidate_data::{CandidateData, CandidateItem, ThemeSnapshot},
    TextSystem,
};
use windows::Win32::{
    Foundation::{CloseHandle, HANDLE},
    System::{ProcessStatus::*, Threading::*},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

struct Process {
    pid: u32,
    handle: HANDLE,
}
impl Process {
    fn open(pid: u32) -> Result<Self> {
        // Query-only access; the probe cannot inject input or modify another process.
        let handle =
            unsafe { OpenProcess(PROCESS_QUERY_INFORMATION | PROCESS_VM_READ, false, pid)? };
        Ok(Self { pid, handle })
    }
    fn sample(&self) -> Result<Value> {
        let mut memory = PROCESS_MEMORY_COUNTERS_EX {
            cb: std::mem::size_of::<PROCESS_MEMORY_COUNTERS_EX>() as u32,
            ..Default::default()
        };
        let mut handles = 0;
        // The EX structure starts with PROCESS_MEMORY_COUNTERS and cb specifies its full size.
        unsafe {
            GetProcessMemoryInfo(
                self.handle,
                (&mut memory as *mut PROCESS_MEMORY_COUNTERS_EX).cast(),
                memory.cb,
            )?;
            GetProcessHandleCount(self.handle, &mut handles)?;
            Ok(
                json!({"pid":self.pid,"private_bytes":memory.PrivateUsage,"working_set":memory.WorkingSetSize,
                "handles":handles,"gdi":GetGuiResources(self.handle,GR_GDIOBJECTS),"user":GetGuiResources(self.handle,GR_USEROBJECTS)}),
            )
        }
    }
}
impl Drop for Process {
    fn drop(&mut self) {
        unsafe {
            let _ = CloseHandle(self.handle);
        }
    }
}
fn distribution(mut ns: Vec<u64>) -> Value {
    ns.sort_unstable();
    json!({"samples":ns.len(),"p50_ns":ns[(ns.len()-1)/2],"p95_ns":ns[(ns.len()*95).div_ceil(100)-1],"p99_ns":ns[(ns.len()*99).div_ceil(100)-1]})
}
fn appearance(path: &Path, config: &Config) -> Result<ThemeSnapshot> {
    let skin = runtime::load(path, config, "windows", false, false)?;
    let cache = match &skin.package {
        Some(p) => text_render::skin_images::decode(p, &skin.images)?,
        None => Default::default(),
    };
    let mut theme = if skin.legacy {
        ThemeSnapshot::from_config(config)
    } else {
        ThemeSnapshot::from_parameters(skin.parameters)
    };
    theme.images = Arc::new(cache);
    Ok(theme)
}
fn measure(config_path: &Path, output: &Path) -> Result<()> {
    let mut config = Config::load(config_path)?;
    let states: Value =
        serde_json::from_str(include_str!("../../../docs/validation/skins/states.json"))?;
    let mut system = TextSystem::new()?;
    let process = Process::open(std::process::id())?;
    let mut results = Vec::new();
    for preset in ["legacy", "jade", "plain", "contrast", "bamboo"] {
        for dark in [false, true] {
            if preset == "legacy" && dark {
                continue;
            }
            config.skins.windows.preset = if preset == "bamboo" { "jade" } else { preset }.into();
            config.skins.windows.mode = if dark { Mode::Dark } else { Mode::Light };
            config.skins.windows.selection = (preset == "bamboo").then(|| Selection {
                id: "mywubi.bamboo".into(),
                version: "1".into(),
                component: "jade".into(),
            });
            let load_start = Instant::now();
            let theme = appearance(config_path, &config)?;
            let load_ns = load_start.elapsed().as_nanos() as u64;
            for state in states["states"].as_array().ok_or("states missing")? {
                let data = CandidateData {
                    visible: true,
                    spelling: state["code"].as_str().ok_or("code missing")?.into(),
                    theme: theme.clone(),
                    items: vec![CandidateItem {
                        label: "".into(),
                        text: state["text"].as_str().ok_or("text missing")?.into(),
                        hint: "".into(),
                        disabled: false,
                        expression_kind: None,
                    }],
                    ..Default::default()
                };
                let mut samples = Vec::with_capacity(200);
                for n in 0..220 {
                    let start = Instant::now();
                    let bitmap = text_render::preview::candidate(&mut system, &data, 600.0)?;
                    std::hint::black_box(bitmap);
                    if n >= 20 {
                        samples.push(start.elapsed().as_nanos() as u64);
                    }
                }
                results.push(json!({"preset":preset,"dark":dark,"state":state["id"],"load_decode_ns":load_ns,"render":distribution(samples),"resources":process.sample()?}));
            }
        }
    }
    // Exercise destruction of replaced package, decoded pixels, layouts and native bitmaps.
    let mut curve = Vec::new();
    let mut loads = Vec::new();
    for n in 0..1020 {
        config.skins.windows.mode = if n % 2 == 0 { Mode::Light } else { Mode::Dark };
        let start = Instant::now();
        let theme = appearance(config_path, &config)?;
        let data = CandidateData {
            visible: true,
            spelling: "a".into(),
            theme,
            items: vec![CandidateItem {
                label: "1. ".into(),
                text: "竹纸图片换肤资源回收 👩🏽‍💻".into(),
                hint: "aaaa".into(),
                disabled: false,
                expression_kind: None,
            }],
            ..Default::default()
        };
        std::hint::black_box(text_render::preview::candidate(&mut system, &data, 600.0)?);
        drop(data);
        if n >= 20 {
            loads.push(start.elapsed().as_nanos() as u64);
        }
        if n >= 19 && (n + 1) % 10 == 0 {
            curve.push(json!({"switch":n+1-20,"resources":process.sample()?}));
        }
    }
    fs::write(
        output,
        serde_json::to_vec_pretty(
            &json!({"scope":"96 DPI native layout/raster and load/decode, not TSF/display latency","warmup":20,"results":results,"reload_render":distribution(loads),"resource_curve":curve}),
        )?,
    )?;
    Ok(())
}
fn stress(config_path: &Path, log_path: &Path, output: &Path, pids: &[u32]) -> Result<()> {
    let original = Config::load(config_path)?;
    let mut config = original.clone();
    let processes: Vec<_> = pids
        .iter()
        .map(|pid| Process::open(*pid))
        .collect::<Result<_>>()?;
    let result = (|| -> Result<()> {
        let mut log = fs::File::open(log_path)?;
        let mut stream = fs::File::create(output.with_extension("jsonl"))?;
        let mut curve = Vec::new();
        let mut latencies = Vec::new();
        let start = Instant::now();
        for n in 0..1020 {
            log.seek(SeekFrom::End(0))?;
            let code = 0xff203000u32 + n;
            config.skins.windows.mode = if n % 2 == 0 { Mode::Light } else { Mode::Dark };
            config.skins.windows.overrides.insert(
                "code".into(),
                core_engine::skin::parameters::argb_to_color(code).into(),
            );
            let changed = Instant::now();
            config.save(config_path)?;
            let markers: Vec<_> = pids
                .iter()
                .map(|pid| format!("pid={pid} code={code:08X}"))
                .collect();
            let mut received = String::new();
            loop {
                let mut bytes = Vec::new();
                log.read_to_end(&mut bytes)?;
                received.push_str(&String::from_utf8_lossy(&bytes));
                if markers.iter().all(|marker| received.contains(marker)) {
                    break;
                }
                if changed.elapsed() > Duration::from_secs(15) {
                    return Err(format!(
                        "snapshot acknowledgement timeout at {n}: {}",
                        markers
                            .iter()
                            .filter(|m| !received.contains(*m))
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(",")
                    )
                    .into());
                }
                thread::sleep(Duration::from_millis(10));
            }
            if n >= 20 {
                latencies.push(changed.elapsed().as_nanos() as u64);
            }
            thread::sleep(Duration::from_millis(35));
            if n >= 19 && (n + 1) % 10 == 0 {
                let row = json!({"switch":n+1-20,"elapsed_ms":start.elapsed().as_millis(),"processes":processes.iter().map(Process::sample).collect::<Result<Vec<_>>>()?});
                writeln!(stream, "{row}")?;
                stream.flush()?;
                curve.push(row);
            }
            if n == 519 {
                fs::write(
                    output.with_extension("pause"),
                    b"500 acknowledged switches; change active host then create .resume",
                )?;
                while !output.with_extension("resume").exists() {
                    thread::sleep(Duration::from_millis(100));
                }
            }
        }
        for n in 0..10 {
            thread::sleep(Duration::from_secs(1));
            curve.push(json!({"settle_second":n+1,"processes":processes.iter().map(Process::sample).collect::<Result<Vec<_>>>()?}));
        }
        fs::write(
            output,
            serde_json::to_vec_pretty(
                &json!({"switches":1000,"warmup":20,"acknowledged_pids":pids,"scope":"config save to all loader publications; not key-to-visible latency","reload":distribution(latencies),"curve":curve}),
            )?,
        )?;
        Ok(())
    })();
    original.save(config_path)?;
    result
}
fn main() -> Result<()> {
    let args: Vec<_> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("measure") if args.len() == 4 => measure(Path::new(&args[2]), Path::new(&args[3])),
        Some("stress") if args.len() >= 6 => {
            let pids = args[5..]
                .iter()
                .map(|p| p.parse())
                .collect::<std::result::Result<Vec<u32>, _>>()?;
            stress(
                Path::new(&args[2]),
                Path::new(&args[3]),
                Path::new(&args[4]),
                &pids,
            )
        }
        _ => {
            Err("usage: skin_probe measure CONFIG OUTPUT | stress CONFIG LOG OUTPUT PID...".into())
        }
    }
}
