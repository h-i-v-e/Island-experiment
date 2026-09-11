# Ocean seabed blend validation — 2026-09-12

- Native sea-mask tests: 6 passed; encoding now covers 0–10 metres.
- Native generation export test: passed; support, render and tiled meshes retain geometry below the previous 5-metre floor and clip at 10 metres.
- `cargo fmt --check` and `cargo clippy --locked --no-default-features --lib -- -D warnings`: passed.
- Unity 6000.5.6f1, Metal, isolated project: 11 tests passed (`results.xml`). Includes ocean optics, river optics and GPU buoyancy sampling.
- The rendered cutoff test uses the actual sea shader with absorption disabled and a bright green bed. It checks full visibility at 5 m, half visibility at 7.75 m, and a match to absent geometry at 9.5/10 m. Tested top-down/oblique perspective and orthographic cameras, sea levels 0/4 m, and refraction enabled.
- `ocean-ten-metre-cutoff.png` intentionally shows only deep-water colour: the green bed at 10 m has faded completely.
- GPU mask checks verify linear 10-metre decoding while ordinary swell retains its 5-metre attenuation range. Existing breaker thresholds are preserved; authored full breaking is 4 m, fallback 3.5 m.
- Built and installed the release macOS plugin with `island-rs/deploy-unity.sh`; SHA-256 `a78022a9d89362f0c0f60d536b5cd561c7d3ffcb8fc192b152cd04681a7632b6`.

These are controlled rendering and native export checks, not visual approval of the generated island scene. Restart Unity to load the new native plugin and regenerate the scene before judging the coastline transition.
