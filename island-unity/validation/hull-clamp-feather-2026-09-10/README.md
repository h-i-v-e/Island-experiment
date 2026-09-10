# Inset and feathered hull clamp

Four focused tests passed without skips in isolated Unity 6000.5.6f1 on Metal.

The GPU profile test uses a rectangular black/neutral hull stamp and compares
scale 1 / feather 0, scale 0.9 / feather 0, and scale 0.9 / feather 3 metres.
It verifies the footprint moves inward, the strongest adjacent-sample change
drops by at least 20%, the centre flattens fully and the outside stays unchanged.
The existing hull/bow/wake test confirms positive displacement and wake behaviour
remain intact; deck and displacement-foam tests also pass.

Existing texture assets and user scene settings were not edited. No live-scene
appearance check, commit or push was performed.

## Upper-height mask

The mask now defines a ceiling instead of attenuating all positive waves. Five
focused checks passed (`unity-ceiling-tests.xml`): hull inset/feather profile,
displacement foam and ceiling behaviour, authored ship/wake field, deck clamp,
and translucency lighting. Added GPU assertions cover positive waves below the
ceiling, waves exactly at it, negative troughs and taller crests crossing it.
Unchanged waves add no clamp foam. Full mask still clips positive waves to sea
level; zero mask retains normal waves and bow raise. No live scene was restarted.
