# kaomoji-data

> 1,700+ Japanese kaomoji (顔文字) organized by mood and scene — the open dataset behind [kaomojikan.com](https://kaomojikan.com).
>
> 気分・シーン別にまとめた日本語の顔文字データセット。[顔文字館](https://kaomojikan.com) で使われているデータを、そのまま開放しています。

**Last updated**: <!-- lastUpdated:start -->2026-06-08<!-- lastUpdated:end --> · **License**: MIT · **Live demo**: [kaomojikan.com](https://kaomojikan.com)

---

## What's inside

<!-- stats:start -->
| Category | Slug | Count |
|----------|------|-------|
| 可愛い | `cute` | 303 |
| 泣く | `cry` | 137 |
| なでなで | `nadenade` | 80 |
| チラッ | `chira` | 78 |
| コピペ | `copype` | 88 |
| 投げる | `nageru` | 50 |
| もぐもぐ | `mogumogu` | 48 |
| 動物 | `animal` | 109 |
| 嬉しい | `ureshii` | 78 |
| 照れる | `tereru` | 97 |
| びっくり | `bikkuri` | 74 |
| 絵文字ミックス | `emoji` | 177 |
| 量産型 | `ryosangata` | 152 |
| 笑顔 | `egao` | 90 |
| キラキラ | `kirakira` | 75 |
| オタク | `otaku` | 70 |
| 好き | `suki` | 78 |
| てへぺろ | `tehepero` | 78 |
| 倒れる | `taoreru` | 70 |
| AA | `aa` | 58 |
| 眠い | `nemui` | 50 |
| 怒る | `okoru` | 51 |
| 応援 | `ouen` | 58 |
| **Total** | | **1808** |
<!-- stats:end -->

> Some kaomoji appear in more than one category, so per-category counts can overlap and the **Total** is the number of entries across all categories (not deduplicated by kaomoji text).

- `kaomoji.json` — flat array of all kaomoji entries
- `categories.json` — category metadata (name, reading, synonyms, representative kaomoji)
- `by-category/<slug>.json` — kaomoji grouped per category
- `stats.json` — counts and last-updated timestamp

---

## Usage

### CDN (browser / Deno)

```js
const data = await fetch(
  "https://raw.githubusercontent.com/kaomojikan/kaomoji-data/main/kaomoji.json"
).then((r) => r.json());
```

Or via jsDelivr (cached, faster):

```js
const data = await fetch(
  "https://cdn.jsdelivr.net/gh/kaomojikan/kaomoji-data@main/kaomoji.json"
).then((r) => r.json());
```

### Node.js

```js
import data from "./kaomoji.json" assert { type: "json" };
const cute = data.filter((k) => k.categories.includes("cute"));
```

### Python

```python
import json, urllib.request
url = "https://raw.githubusercontent.com/kaomojikan/kaomoji-data/main/kaomoji.json"
data = json.loads(urllib.request.urlopen(url).read())
cute = [k for k in data if "cute" in k["categories"]]
```

---

## Schema

### Kaomoji entry (`kaomoji.json`, `by-category/*.json`)

```ts
type Kaomoji = {
  text: string;        // 顔文字本体 e.g. "(=^・ω・^=)"
  slug: string;        // opaque ID, e.g. "animal-0001"
  categories: string[]; // category slugs e.g. ["animal", "cute"]
  tags: string[];      // Japanese tags e.g. ["動物", "ねこ"]
  reading: string[];   // hiragana readings e.g. ["にゃー", "ねこ"]
};
```

**Notes**
- `reading` is **hiragana only**. Consumers building search should normalize hiragana ↔ katakana on the query side.
- `text` itself is searchable — users can paste a kaomoji to find related entries.
- `slug` is opaque; do not parse the trailing number as an ordering key.

### Category metadata (`categories.json`)

```ts
type Category = {
  slug: string;       // e.g. "cute"
  urlSlug: string;    // URL on kaomojikan.com, e.g. "cute-kaomoji"
  name: string;       // 日本語名 e.g. "可愛い"
  reading: string;    // hiragana e.g. "かわいい"
  synonyms: string[]; // search aliases (JP + English)
  axis: string;       // categorization axis: "style" | "emotion" | "gesture" | "copy"
  mascot: string;     // mascot expression ID used on the category page
  repr: string;       // representative kaomoji
  lead: string;       // 1-line category intro (Japanese)
};
```

---

## Categories

Browse the live, copy-ready pages on [kaomojikan.com](https://kaomojikan.com):

| Slug | 日本語名 | 代表 | Live page |
|------|------|------|------|
| `cute` | 可愛い | (｡♡‿♡｡) | [kaomojikan.com/cute-kaomoji](https://kaomojikan.com/cute-kaomoji) |
| `cry` | 泣く | (╥﹏╥) | [kaomojikan.com/cry-kaomoji](https://kaomojikan.com/cry-kaomoji) |
| `ureshii` | 嬉しい | (◍•ᗜ•◍) | [kaomojikan.com/ureshii-kaomoji](https://kaomojikan.com/ureshii-kaomoji) |
| `tereru` | 照れる | (*ノωノ) | [kaomojikan.com/tereru-kaomoji](https://kaomojikan.com/tereru-kaomoji) |
| `bikkuri` | びっくり | Σ(ﾟДﾟ) | [kaomojikan.com/bikkuri-kaomoji](https://kaomojikan.com/bikkuri-kaomoji) |
| `animal` | 動物 | (=^･ω･^=) | [kaomojikan.com/animal-kaomoji](https://kaomojikan.com/animal-kaomoji) |
| `nadenade` | なでなで | ヾ(´ω｀)ﾉﾅﾃﾞﾅﾃﾞ | [kaomojikan.com/nadenade-kaomoji](https://kaomojikan.com/nadenade-kaomoji) |
| `mogumogu` | もぐもぐ | (っ˘ڡ˘ς) | [kaomojikan.com/mogumogu-kaomoji](https://kaomojikan.com/mogumogu-kaomoji) |
| `nageru` | 投げる | (ノ°Д°)ノ︵┻━┻ | [kaomojikan.com/throw-kaomoji](https://kaomojikan.com/throw-kaomoji) |
| `chira` | チラッ | \|ω・) | [kaomojikan.com/chirachira-kaomoji](https://kaomojikan.com/chirachira-kaomoji) |
| `copype` | コピペ | ¯\\\_(ツ)\_/¯ | [kaomojikan.com/copy-paste-kaomoji](https://kaomojikan.com/copy-paste-kaomoji) |
| `emoji` | 絵文字ミックス | ✨(ﾉ◕ヮ◕)ﾉ✨ | [kaomojikan.com/emoji-kaomoji](https://kaomojikan.com/emoji-kaomoji) |

---

## Versioning

Releases follow SemVer adapted for a dataset:

| Change | Bump |
|------|------|
| Adding kaomoji entries | patch (`v0.1.x`) |
| New category, new schema field | minor (`v0.x.0`) |
| Renaming or removing fields | major (`vx.0.0`) |

See [Releases](https://github.com/kaomojikan/kaomoji-data/releases) for changelogs.

---

## Contributing

Found a typo, a 文字化け entry, or have a kaomoji you wish was included? Open an issue or PR. New categories should reference a real Japanese use-case (気分 / シーン / 動作), not be invented from English emoji buckets.

---

## About

Built and maintained as part of [顔文字館 / kaomojikan.com](https://kaomojikan.com) — a small Japanese kaomoji copy-paste site with a mascot 看板娘 (にゃもじ).

- 🌐 Website: [kaomojikan.com](https://kaomojikan.com)
- 🐦 X (Twitter): [@kaomojikan](https://twitter.com/kaomojikan)
- 📝 Dev notes: [note.com/kaomojikan](https://note.com/kaomojikan)

## License

[MIT](./LICENSE) — kaomoji text characters themselves are public-domain Unicode sequences; the curation, categorization, and metadata in this repository are released under MIT.
