<!-- Modified by Jatin Bharati (2026-11-04): APK input and the SDK report, Bootstrap removed (fork-webapp). See NOTICE. -->
# dex-gui

The in-browser front end of [dex-parser-analyzer](../README.md): a client-side Leptos app, compiled to WebAssembly. In this fork it takes an `.apk` or a `.dex` file (the original takes a `.dex` file only) and shows four tabs: the SDKs the app contains with the evidence for each (`dexpa-sdk`), the Manifest, the Findings of every `dexpa-analysis` pass, and the Classes. A "try a sample app" button loads a bundled sample APK. The file is read in the page and never uploaded. Live at https://j18bharati.github.io/dex-parser-analyzer/; the original, which takes DEX files only, is at https://dhilipsiva.github.io/dex-parser-analyzer/.

An APK is read for its archive-root `classes.dex`, `classes2.dex`, ... and its `AndroidManifest.xml`. Limits: 200 MiB per file, 256 MiB per DEX entry, 512 MiB of DEX in total, and 500 rows per table (each table says how many there are). A file that is neither an APK nor a DEX file, or an archive that cannot be read, is an error in the page, and the next file still loads. A bare DEX file has no manifest, so its SDK report rests on class names alone.

## Local development

```bash
nix develop
cd dex-gui && trunk serve
```

Open `http://localhost:8080` and pick an `.apk` or `.dex` file, or use the sample. The dev shell provides trunk, the wasm32 target and everything else (trunk 0.21 from the flake's nixpkgs).

## The release bundle

`nix build .#dex-gui` builds what GitHub Pages serves: `index.html`, the JS glue, the wasm module and one stylesheet, all with relative URLs, so the bundle works under any path.

- Nothing loads from a third party. `assets/app.css` is the only stylesheet, and the fonts are the system's. Bootstrap, which the original vendors, is not used.
- A release build carries a Content-Security-Policy `<meta>` tag: `default-src 'none'`, scripts from the page's origin plus trunk's inline loader by hash, `'wasm-unsafe-eval'` to compile the module, and styles from the origin only, so the view code uses classes, never `style` attributes. `csp.py`, a trunk `post_build` hook, writes it, because the loader's hash is known only after trunk has written it. `trunk serve` builds are left without a policy, so trunk's autoreload keeps working.

`.github/workflows/deploy-dex-gui.yml` builds the bundle with the flake and deploys it on every push to `main` that touches the GUI, `dex-core`, `dex-analysis`, `dex-sdk`, the lockfiles or the toolchain pins, and after each release.

## Tests

`nix develop .#wasm-test -c just test-wasm` runs the browser smoke tests in `src/smoke.rs` with wasm-bindgen-test in headless Chromium: the app mounts, and the analyzer, compiled to wasm32, reports and renders the findings of the rule fixture. `src/smoke_fork.rs` holds the fork's tests: the sample app's SDK report in wasm against the CLI's golden file, and the tabs. `src/smoke.rs` is unedited. CI runs them in the `gui-smoke` job.
