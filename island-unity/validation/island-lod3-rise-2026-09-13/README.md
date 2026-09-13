# LOD3 colour and emergence

Distant island silhouettes use the unmodified atmospheric haze colour, matching fully hazed distant geometry. World Environment Settings / Sky Horizon Lightening (default 0.08) brightens only the sky horizon by a factor of 1.08, before its existing exposure and elevation gradient. Zero matches haze and one doubles sky horizon brightness. The sky camera background follows the lifted colour. The former island-darkening setting is migrated to the new control when serialized. The environment updates it live alongside time-of-day horizon colour.

The LOD3 visual root is lowered by the generated island's configured maximum height at 4,000 m and farther, half that height at 3,000 m, and zero at 2,000 m and nearer. Smoothstep eases both endpoints. Viewer distance is measured from the original island centre, including altitude; moving away reverses the rise. Only the LOD3 child moves, including while dormant.

Validation: Unity 6000.5.6f1, isolated project, Metal, EditMode. All 6 selected tests passed in 67.35 seconds:
- GPU pixel checks for unmodified day/night island haze colours, with ambient lighting and fog enabled.
- Native generation/cancellation/installation/unload, including continuous far-distance motion, reverse movement, dormant updates, translated/elevated centres, return to tiled LODs, and unchanged island centre.
- At midnight and noon, sky lightening values 0, 0.08, 0.2 and 1 preserve haze, LOD3 colour and zenith colour while changing only the exposed sky horizon brightness.
- Runtime ownership and supported scenes.

`git diff --check` passed. No native plugin changes were needed. Interactive scene appearance has not been visually reviewed in the user's editor.
