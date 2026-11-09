# kaomoji-collection

The largest curated collection of Japanese kaomoji (顔文字) available as an npm package.

**41,000+ kaomoji** across **535 categories** — sourced from [kaomojiya.org (顔文字屋)](https://www.kaomojiya.org), the most comprehensive kaomoji reference on the web.

## Install

```bash
npm install kaomoji-collection
```

## Usage

```javascript
const kaomoji = require('kaomoji-collection');

// Get a random kaomoji
kaomoji.random();        // → (◕‿◕✿)

// Get a random kaomoji from a category
kaomoji.random('smile'); // → (﹡ˆ﹀ˆ﹡)
kaomoji.random('cat');   // → (^≗ω≗^)
kaomoji.random('angry'); // → (╬ Ò﹏Ó)

// List all kaomoji in a category
kaomoji.list('dog');     // → ['▼・ᴥ・▼', 'U・ᴥ・U', ...]

// Search categories
kaomoji.search('happy'); // → { happy: [...], ... }

// Get all category names
kaomoji.categories();    // → ['smile', 'cat', 'dog', ...]

// Collection stats
kaomoji.stats();
// → { categories: 535, total: 41596, largest: 'cute' }
```

### ESM / TypeScript

```typescript
import { random, list, search, categories, stats } from 'kaomoji-collection';

const face: string = random('kawaii');
const allSmiles: string[] = list('smile');
```

## Categories

535 categories covering every emotion and situation:

| Category | Example | Count |
|----------|---------|-------|
| smile | (╹◡╹) | — |
| cat | (=^・ω・^=) | — |
| dog | ▼・ᴥ・▼ | — |
| angry | (╬ Ò﹏Ó) | — |
| cry | (T_T) | — |
| love | (♡˙︶˙♡) | — |
| shy | (*/ω＼*) | — |
| cute | (◕‿◕✿) | — |
| happy | ヽ(>∀<☆)ノ | — |
| surprise | Σ(°△°) | — |

[Browse all 535 categories on kaomojiya.org →](https://www.kaomojiya.org)

## What are Kaomoji?

Kaomoji (顔文字) are Japanese-style emoticons made from Unicode characters. Unlike Western emoticons (`:-)`) which are read sideways, kaomoji are read straight-on and can express a much wider range of emotions.

```
Western:  :-)    :-(    ;-)    :D
Kaomoji:  (◕‿◕)  (╥_╥)  (^_~)  (≧▽≦)
```

They originated in Japanese internet culture and are now used worldwide in messaging, social media, and creative writing.

[Learn more about kaomoji →](https://www.kaomojiya.org)

## Data

All kaomoji data is available as JSON:

- **`kaomoji.json`** — Complete collection (all categories)
- **`categories/*.json`** — Individual category files

### Direct JSON usage

```javascript
const data = require('kaomoji-collection/kaomoji.json');

// data = { smile: ['☺︎', '⑅◡̈*', ...], cat: ['(=^・ω・^=)', ...], ... }
```

## Source

All kaomoji are curated by [**顔文字屋 (kaomojiya.org)**](https://www.kaomojiya.org) — the largest Japanese kaomoji reference site with 500+ categorized pages.

- 🌐 Website: [kaomojiya.org](https://www.kaomojiya.org)
- 📦 npm: [kaomoji-collection](https://www.npmjs.com/package/kaomoji-collection)
- 🐙 GitHub: [kaomojiya-collection/kaomoji-collection](https://github.com/kaomojiya-collection/kaomoji-collection)

## License

MIT © [Kaomojiya](https://www.kaomojiya.org)
