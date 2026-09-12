# Visible-wave foam cutoff — 2026-09-12

The earlier amplitude-envelope filter was insufficient. It could admit foam despite tiny or zero rendered wave heights.

Regression validation in an isolated Unity 6000.5.6f1 Metal project:

- With the previous height gating, both new checks fail as expected (`previous-cutoff-expected-failures.xml`): six-centimetre waves produce foam of 0.137755, and a flat rendered surface with stored foam differs from its foam-free baseline by 1.744323 in summed RGB.
- With the new gate, all 22 ocean optics/translucency/foam/ship-wave/buoyancy checks pass (`results.xml`). A 17.5 cm crest exercises the smooth transition.
- Ambient whitecaps/history require actual crest height over 10 cm and fade to full allowance at 25 cm. The check runs on the combined depth-limited wave and on the final rendered mesh height. New/stored foam cannot bypass it via a large authored amplitude envelope.
- The controlled render (`flat-ocean-no-foam.png`) deliberately retains large analytic waves and fills the history texture with foam while flattening the mesh. Its pixels match the foam-free baseline within 0.001 summed RGB.
- Active boat-generated contact foam remains separate. Very shallow breakers now fade out once their actual crest becomes too small.

This reproduces concrete failures of the previous cutoff; it does not claim visual acceptance in the user's live island scene. No native rebuild is required.
