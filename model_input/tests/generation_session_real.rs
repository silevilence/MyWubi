#![cfg(feature = "llama-cpp-2")]
use core_engine::{config::*, runtime::RuntimeSnapshot, session::*, *};
use model_input::worker::{Phase, Worker};
use std::{
    path::PathBuf,
    sync::Arc,
    time::{Duration, Instant},
};

/// Exercises the same worker/deadline/publication boundary as both shells.
#[test]
#[ignore = "requires MYWUBI_TEST_MODEL_STORE with qwen25-words--v1"]
fn prepared_generation_reaches_the_confirmed_session_before_its_deadline() {
    let root = PathBuf::from(std::env::var("MYWUBI_TEST_MODEL_STORE").unwrap());
    let config = Config {
        prediction: PredictionConfig {
            enabled: true,
            wait_ms: 1000,
            max_candidates: 1,
            loading: ModelLoading::PreloadKeep,
            model: Some(ModelSelection {
                id: "qwen25-words".into(),
                version: "v1".into(),
            }),
            ..Default::default()
        },
        ..Default::default()
    };
    let worker = Worker::new(root, config.prediction.clone()).unwrap();
    let prepare = Instant::now();
    while worker.phase() != Phase::Ready && prepare.elapsed() < Duration::from_secs(30) {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(worker.phase(), Phase::Ready);
    let runtime = RuntimeSnapshot {
        config,
        dict: Dictionary::from_text("v\t北京\t100\n").unwrap(),
        revision: 1,
        temporary: Arc::default(),
        expressions: Arc::default(),
        config_path: "c".into(),
        system_table_path: "s".into(),
        user_table_path: "u".into(),
    };
    for profile in [BehaviorProfile::LegacyV1, BehaviorProfile::TouchV1] {
        let mut session = Session::new(&runtime, profile, HostContext::default());
        let edit = session
            .prepare(Action::InsertLiteral("北京".into()))
            .unwrap();
        assert_eq!(
            session.acknowledge(edit.token, HostOutcome::Applied).status,
            AckStatus::Published
        );
        let request = session.prediction_request().unwrap();
        let key = request.key;
        let started = Instant::now();
        worker.submit(request).unwrap();
        let mut received = None;
        while started.elapsed() < Duration::from_millis(1000) {
            if let Some(result) = worker.take(key) {
                received = Some(result);
                break;
            }
            std::thread::sleep(Duration::from_millis(2));
        }
        eprintln!(
            "{profile:?}: elapsed={:?}, phase={:?}, response={}",
            started.elapsed(),
            worker.phase(),
            serde_json::to_string(&received).unwrap()
        );
        assert!(session.complete_model_prediction(
            key,
            received.expect("prepared worker produced no response before deadline")
        ));
        assert!(
            session.has_prediction(),
            "confirmed host must see the generated prediction"
        );
        assert_eq!(session.candidate_window(0, 1).candidates[0].text, "市");
    }
}

#[test]
#[ignore = "requires MYWUBI_TEST_MODEL_STORE with qwen25-words--v1"]
fn resource_pressure_cancels_old_work_and_rebuilds_without_retaining_host_context() {
    use core_engine::assistance::*;
    use model_input::service::{Command, Endpoint};
    let endpoint = Endpoint::default();
    let call = |command: Command| -> serde_json::Value {
        serde_json::from_slice(&endpoint.exchange(&serde_json::to_vec(&command).unwrap())).unwrap()
    };
    let root = PathBuf::from(std::env::var("MYWUBI_TEST_MODEL_STORE").unwrap());
    let mut config = PredictionConfig {
        enabled: true,
        wait_ms: 1000,
        max_candidates: 1,
        loading: ModelLoading::PreloadKeep,
        model: Some(ModelSelection {
            id: "qwen25-words".into(),
            version: "v1".into(),
        }),
        ..Default::default()
    };
    let wait = |phase: &str| {
        let start = Instant::now();
        while call(Command::Status)["value"] != phase {
            assert!(
                start.elapsed() < Duration::from_secs(30),
                "missing phase {phase}"
            );
            std::thread::sleep(Duration::from_millis(5));
        }
    };
    assert_eq!(
        call(Command::Prepare {
            root: root.clone(),
            config: config.clone()
        })["ok"],
        true
    );
    wait("Ready");
    let mut request: ScoreRequest =
        serde_json::from_str(include_str!("../evaluation/request.json")).unwrap();
    request.task = Task::Predict;
    request.prefix = "北京".into();
    request.code.clear();
    request.candidates.clear();
    call(Command::Submit {
        request: request.clone(),
    });
    // Exercise the exact callback used by the OS memory-pressure monitor,
    // without exhausting the machine or substituting a fake model.
    endpoint.resource_pressure(true);
    wait("Paused");
    assert!(call(Command::Poll { key: request.key })["value"].is_null());
    endpoint.resource_pressure(false);
    wait("Ready");
    assert!(call(Command::Poll { key: request.key })["value"].is_null());
    request.key.request += 1;
    call(Command::Submit {
        request: request.clone(),
    });
    let start = Instant::now();
    loop {
        let response = call(Command::Poll { key: request.key });
        if !response["value"].is_null() {
            let result: Result<PredictionResponse, Unavailable> =
                serde_json::from_value(response["value"].clone()).unwrap();
            assert!(matches!(result.unwrap(),PredictionResponse::Proposed(r) if r.words==["市"]));
            break;
        }
        assert!(start.elapsed() < Duration::from_millis(1000));
        std::thread::sleep(Duration::from_millis(2));
    }
    config.enabled = false;
    call(Command::Prepare { root, config });
    wait("Idle");
    endpoint.resource_pressure(true);
    endpoint.resource_pressure(false);
    wait("Idle");
    assert!(!endpoint.keep_alive());
}
