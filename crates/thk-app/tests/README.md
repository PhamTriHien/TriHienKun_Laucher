# Visual regression (Windows)

Run from the workspace root:

```powershell
cargo test -p thk-app visual_ -- --test-threads=1
```

The tests render the production server header/command controls, profile hero and tabs
using `egui_kittest` with wgpu. They do not initialize `LauncherAppState`, access
the user's database, start a server, or execute backend actions. The server is an
explicitly synthetic fixture, not evidence of a live server connection.

The 22 images cover Vietnamese/English, offline/online server controls,
idle/running profile actions, 450/710-point content widths, and full-workspace
Mobile Portrait (412×892) + Landscape (960×440) adaptive layouts. Rendering uses
Windows, Light/Emerald (the default Bright Game-Engine theme), scale 1.0, fixed
starting time and the application's system-font loader. Pixel comparison is
exact: threshold 0.0, zero failed pixels allowed.
The hero reserves the real preview's 142×154-point geometry; it does not render
or verify the 3D skin. Assertions check hero bounds, single-line badges and
action separation. Play/Stop and Logs clicks exercise UI intent only, not backend
launch/stop behavior.
Font files, GPU backend/driver and OS changes can affect results across machines;
these baselines are not a claim of cross-platform pixel identity.

The three `visual_*` snapshot tests share a render lock (`VISUAL_RENDER_LOCK`). Concurrent snapshot rendering crashed
inside Windows `d3d10warp.dll` with `STATUS_ACCESS_VIOLATION` on the test machine;
serial rendering passed. This keeps other workspace tests parallel without changing
the image comparison or requiring a global test-thread setting.

Alongside the three golden snapshot tests, `crates/thk-app/src/views/visual_tests.rs`
(mounted as `views::servers::visual_tests`) and `crates/thk-app/src/views/home_tests.rs`
contain companion `egui_kittest` bounds, scroll, and AccessKit semantic tests covering
left-rail navigation (`Role::Button` + `AriaCurrent`), Profile General form scrolling
and isolated SQLite persistence, Accounts list scrolling/action separation, Loopback PKCE
disclosure `Expand`/`Collapse`/`Click` handling across widths, and Modpack list/grid cards:

```powershell
cargo test -p thk-app views::servers::visual_tests
cargo test -p thk-app views::home::tests
```

## Reviewing a visual change

1. Run without `UPDATE_SNAPSHOTS`. Inspect every generated `.new.png` and any
   `.diff.png`; also inspect the running desktop application at minimum/default
   window sizes. A passing bounds assertion alone does not prove good layout.
2. Fix unintended changes before updating a baseline. Never accept a failing
   image just to make the test pass, and do not loosen tolerances without
   measuring the source of nondeterminism.
3. For an intentional, reviewed change only:

   ```powershell
   $env:UPDATE_SNAPSHOTS = '1'
   cargo test -p thk-app visual_ -- --test-threads=1
   Remove-Item Env:UPDATE_SNAPSHOTS
   ```

4. Run the normal command at least three times on the same environment. Review
   the baseline diff together with the code change. Only baseline `.png` files
   belong in version control; failure artifacts are ignored.

These component snapshots complement desktop testing. They do not cover the
whole launcher, Settings, network services, authentication, or Minecraft runtime.
