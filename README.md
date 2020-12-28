# pilka (WIP)

![](boring_tunnel.png)

Available features:
 - [X] Hot-reload
 - [ ] Saving shaders
 - [ ] Taking screenshot
 - [ ] Record video
 
Pilka is a cross-platform democoding tool for creating shader* demos, similar to [Bonzomatic](https://github.com/Gargaj/Bonzomatic) or [KodeLife](https://hexler.net/products/kodelife). Supports hot-reloading which means resources is checked, updated in the background.

## Controls
- `F1`:  Toggles play/pause
- `F2`:  Pauses and steps back one frame
- `F3`:  Pauses and steps forward one frame
- `F4`:  Restarts playback at frame 0 (Time and Pos = 0)
- `F9`:  Save shaders
- `F10`: Start/Stop record
- `F11`: Take Screenshot
- `ESC`: Exit the application

Used nightly features which is completely unnecessary and have to be removed:
 - [std::sync::SyncLazy](https://doc.rust-lang.org/std/lazy/struct.SyncLazy.html)

Places from where i steal code:
 - [piet-gpu](https://github.com/linebender/piet-gpu)
 - [Aetna's tutorial](https://hoj-senna.github.io/ashen-aetna/)
 - https://github.com/w23/OpenSource
