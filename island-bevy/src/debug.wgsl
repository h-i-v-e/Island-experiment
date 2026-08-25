#define_import_path island_bevy::debug

// The diagnostic channels `--debug-view` switches the terrain and water
// fragment stages to. The numbering here is `capture::DebugView::flag`'s own and
// the two lists are only correct read together.
//
// A surface answers the views that name a channel it carries and shades
// normally under the rest, so a terrain channel is still seen through the water
// standing over it and a water channel still stands on shaded ground.

const OFF: u32 = 0u;
const WEIGHTS: u32 = 1u;
const WETNESS: u32 = 2u;
const SLOPE: u32 = 3u;
const FLOW: u32 = 4u;
const GRADE: u32 = 5u;
const DEPTH: u32 = 6u;
const STATE: u32 = 7u;
const FOAMLESS: u32 = 8u;
const CHUNKS: u32 = 9u;

/// A scalar channel as a colour: black, blue, green, white. Every capture goes
/// through the same exposure and tone curve the scene does, and a grey ramp
/// under one loses most of its bottom half; four stops this far apart stay
/// apart.
fn ramp(value: f32) -> vec3<f32> {
    let level = clamp(value, 0.0, 1.0);
    var colour = mix(
        vec3<f32>(0.0),
        vec3<f32>(0.10, 0.20, 0.95),
        smoothstep(0.0, 0.34, level),
    );
    colour = mix(colour, vec3<f32>(0.12, 0.85, 0.20), smoothstep(0.34, 0.67, level));
    return mix(colour, vec3<f32>(1.0), smoothstep(0.67, 1.0, level));
}
