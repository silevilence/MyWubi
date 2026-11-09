Windows final host boundary batch, 2026-11-09
Reuse current final DLL and offline Sandbox app/resource cache; no rebuild.
1. Ordinary Main punctuation comma: commit once, Ctrl+Z once and inspect; if unchanged Ctrl+Z again. Save journal as baseline.
2. Emoji query from English mode: a Space c Tab, assert no text before selection; choose 🚳, inspect returned English state and first/second undo. Save journal.
3. Begin Emoji from English, select a locally, cancel via Esc until idle; preserve host text and return English. Begin Kaomoji afterwards, d Tab; cancel. Save journal.
4. Select existing host text and commit an expression; confirm replacement and original text restoration by undo, no extra output.
5. Capture current DLL/browser/OS versions; close Sandbox immediately after all UI checks, then analyze journals.
Do not infer core duplicate commit from duplicate composition events. Record differences and limits.

Execution notes:
- Windows language-bar English closes the input gate, so its right-click menu is intentionally unavailable. The mode-return cases used the menu's temporary English mode with the gate enabled.
- Both Emoji submission/cancellation and Kaomoji result cancellation restored temporary English without adding host mutations.
- Ordinary comma and Emoji each needed two undo operations; normalized event sequences were identical. No single-undo success is claimed.
- The selection case used ordinary input g Space to create 工, selected it, then k Tab 1 to replace with 🚳. Two undo operations restored 工.
- An initial attempt to paste the seed text into the Sandbox produced only beforeinput and no mutation. The original journal retains it.
- All three journals were copied before stopping this Sandbox; final binaries were unchanged.
