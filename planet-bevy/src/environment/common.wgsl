#define_import_path planet_bevy::environment

// All positions are normalized render-world coordinates. Heights, wavelengths
// and extinction lengths remain in metres, independent of the planet radius.
struct EnvironmentUniform {
    shape: vec4<f32>, weather: vec4<f32>, wind: vec4<f32>,
    sun: vec4<f32>, light: vec4<f32>, air: vec4<f32>, viewer: vec4<f32>, simulation: vec4<f32>,
}
// Global ambient/sky probes cannot distinguish the two planetary hemispheres.
// Fade indirect light by radial solar elevation, keeping daylight unchanged and
// a small night floor. Surface normals must not turn cliffs into false day sides.
fn environment_twilight(lower:f32,upper:f32,elevation:f32)->f32 {
    let t=clamp((elevation-lower)/(upper-lower),0.0,1.0);
    // Zero first and second derivatives avoid visible shoulders at either end.
    return t*t*t*(t*(t*6.0-15.0)+10.0);
}
fn environment_ambient_weight(p:vec3<f32>,parameters:EnvironmentUniform)->f32 {
    let elevation=dot(normalize(p),normalize(parameters.sun.xyz));
    // Broad artistic twilight: full daylight above +8.6 degrees, the existing
    // 4% night floor below -20.5 degrees. Mid-twilight remains at -5.7 degrees.
    return mix(0.04,1.0,environment_twilight(-0.35,0.15,elevation));
}
// Filter the existing illumination rather than adding an emissive red band.
// Sunset deepens from amber into red as solar elevation falls, then releases
// the tint into the unchanged dim night floor at the end of the twilight band.
fn environment_sunset_palette(progress:f32)->vec3<f32> {
    let amber=mix(vec3<f32>(1.0),vec3<f32>(1.14,0.63,0.28),
        environment_twilight(0.0,0.60,progress));
    let red=mix(amber,vec3<f32>(1.18,0.24,0.08),
        environment_twilight(0.45,1.0,progress));
    return red;
}
fn environment_twilight_colour(elevation:f32)->vec3<f32> {
    let progress=1.0-environment_twilight(-0.35,0.15,elevation);
    let remaining_light=environment_twilight(-0.35,-0.18,elevation);
    return mix(vec3<f32>(1.0),environment_sunset_palette(progress),remaining_light);
}
fn environment_surface_tint(p:vec3<f32>,parameters:EnvironmentUniform)->vec3<f32> {
    return environment_twilight_colour(dot(normalize(p),normalize(parameters.sun.xyz)));
}
fn environment_rotate(q: vec4<f32>, p: vec3<f32>) -> vec3<f32> {
    return p + 2.0 * cross(q.xyz, cross(q.xyz, p) + q.w * p);
}
// Current and previous snapshots occupy two sets of six cubes. Only this
// interpolation changes each frame; the persistent image uploads at tick rate.
fn environment_weather_sample(n: vec3<f32>, layer: i32, parameters: EnvironmentUniform,
    weather_map: texture_cube_array<f32>, weather_sampler: sampler) -> vec4<f32> {
    let alpha = clamp(parameters.simulation.x - 1.0, 0.0, 1.0);
    // Uniform endpoint branches avoid sampling two identical-age snapshots
    // while paused, or after an upload has completely interpolated.
    if alpha <= 0.0001 {
        return textureSampleLevel(weather_map, weather_sampler, n, layer + 6, 0.0);
    }
    if alpha >= 0.9999 {
        return textureSampleLevel(weather_map, weather_sampler, n, layer, 0.0);
    }
    let current = textureSampleLevel(weather_map, weather_sampler, n, layer, 0.0);
    let previous = textureSampleLevel(weather_map, weather_sampler, n, layer + 6, 0.0);
    return mix(previous, current, alpha);
}
// shape.y bounds the highest terrain globally; weather.w carries the requested
// minimum over clear lowlands. Keeping both permits a tight global shell while
// individual clouds follow the conservative local terrain ceiling.
fn environment_cloud_bottom(parameters:EnvironmentUniform)->f32 {
    if parameters.weather.w<0.5 {return parameters.shape.y;}
    return min(parameters.shape.y,parameters.weather.w);
}
fn environment_local_cloud_base(parameters:EnvironmentUniform,terrain_height:f32)->f32 {
    let minimum=environment_cloud_bottom(parameters);
    if parameters.shape.y>minimum+0.001 {
        return min(parameters.shape.y,max(minimum,terrain_height+20.0));
    }
    return parameters.shape.y;
}
fn environment_cloud_top(parameters: EnvironmentUniform) -> f32 {
    return parameters.shape.y + parameters.shape.z * select(1.0, 2.5, parameters.simulation.x > 0.5);
}
// Fine billows only retain a short local flow history. Two overlapping
// windows fade out before their coordinates restart, so changing winds never
// multiply displacement by the world's accumulated multi-day clock.
struct CloudFlow { first: vec3<f32>, second: vec3<f32>, blend: f32 }
fn environment_cloud_flow(p: vec3<f32>, n: vec3<f32>, wind: vec3<f32>, parameters: EnvironmentUniform) -> CloudFlow {
    let tangent = wind - n * dot(wind, n);
    let phase = fract(parameters.simulation.y / 1800.0);
    let second_phase = fract(phase + 0.5);
    // A window's drift is a very short arc. Its tangent chord avoids four
    // trigonometric evaluations and heading normalization at every sample.
    let base = p * parameters.shape.x;
    let drift = tangent * (1800.0 / max(parameters.simulation.z, 1.0)) * dot(p, n) * parameters.shape.x;
    let blend = smoothstep(0.0, 1.0, 1.0 - abs(phase * 2.0 - 1.0));
    return CloudFlow(base - drift * phase, base - drift * second_phase, blend);
}

fn environment_coverage_threshold(coverage:f32, simulated:bool)->f32 {
    let fraction=clamp(coverage,0.0001,0.9999);
    return select(1.0-coverage,0.48+0.105*log((1.0-fraction)/fraction),simulated);
}
fn environment_low_cloud(position:vec3<f32>,up:vec3<f32>,altitude:f32,coverage:f32,density:f32,climate:vec4<f32>,parameters:EnvironmentUniform,
    noise:texture_3d<f32>,noise_sampler:sampler)->f32 {
    let scale=max(parameters.shape.w*4.0,1.0);
    let seed=vec3<f32>(0.371,0.619,0.237)*parameters.viewer.w;
    let base_position=position-up*(altitude-parameters.shape.y);
    // Organize small puffs into larger banks. Splitting area into bank and
    // within-bank occupancy preserves the requested mean fraction in the
    // statistical field, while leaving real clear intervals along grazing rays.
    let within_bank=max(coverage,0.65);
    let bank_cover=coverage/within_bank;
    let bank_noise=textureSampleLevel(noise,noise_sampler,base_position/(scale*4.0)+seed+vec3<f32>(0.137,0.413,0.719),0.0).r;
    let bank_threshold=environment_coverage_threshold(bank_cover,parameters.simulation.x>0.5);
    let bank=smoothstep(bank_threshold-0.035,bank_threshold+0.07,bank_noise);
    if bank<=0.002 {return 0.0;}
    let footprint=textureSampleLevel(noise,noise_sampler,base_position/scale+seed,0.0);
    let base_offset=0.02+footprint.b*0.26+(1.0-climate.z)*0.20;
    let storm=smoothstep(0.04,0.30,climate.w);
    let available=select(0.96,2.40,parameters.simulation.x>0.5)-base_offset;
    let span=min(0.85+footprint.r*0.80+storm*0.20,available);
    let height=((altitude-parameters.shape.y)/parameters.shape.z-base_offset)/max(span,0.1);
    if height<=0.0 || height>=1.0 {return 0.0;}
    // Isotropic Worley lobes round the sides as well as the roofs; the varying
    // height envelope clips them into individual rising cloud bodies.
    let point=position+(footprint.gba-0.5)*(scale*0.18);
    let shape=textureSampleLevel(noise,noise_sampler,point/scale+seed,0.0);
    let detail=textureSampleLevel(noise,noise_sampler,point/(scale*0.43)+seed+vec3<f32>(0.17,0.61,0.37),0.0);
    let threshold=max(0.53-storm*0.08,environment_coverage_threshold(within_bank,parameters.simulation.x>0.5));
    let rounded_top=pow(smoothstep(0.65,1.0,height),2.0)*0.12;
    let structure=shape.r*0.45+shape.g*0.55-rounded_top;
    let formation=smoothstep(threshold-0.06,threshold+0.09,structure);
    if formation<=0.001 {return 0.0;}
    let edge=4.0*formation*(1.0-formation);
    let erosion=(1.0-detail.b*0.65-detail.a*0.35)*(0.10+edge*0.24);
    let body=pow(max(formation-erosion,0.0),1.4);
    let bottom=smoothstep(0.0,0.12,height);
    let top=1.0-smoothstep(0.78,1.0,height);
    return body*bottom*top*density*bank*1.5/sqrt(max(span,0.8));
}
fn environment_high_cloud(position:vec3<f32>,up:vec3<f32>,altitude:f32,coverage:f32,density:f32,wind:vec3<f32>,parameters:EnvironmentUniform,
    noise:texture_3d<f32>,noise_sampler:sampler)->f32 {
    let scale=max(parameters.shape.w*4.0,1.0);
    let seed=vec3<f32>(0.371,0.619,0.237)*parameters.viewer.w+vec3<f32>(0.413,0.173,0.619);
    let base_position=position-up*(altitude-parameters.shape.y-parameters.shape.z*1.5);
    let footprint=textureSampleLevel(noise,noise_sampler,base_position/(scale*2.0)+seed,0.0);
    let base=parameters.shape.y+parameters.shape.z*(1.44+footprint.r*0.20);
    let thickness=parameters.shape.z*(0.20+footprint.b*0.32);
    let height=(altitude-base)/max(thickness,0.1);
    if height<=0.0 || height>=1.0 {return 0.0;}
    // Fixed-axis fibre fields are blended by local wind, avoiding seams from
    // constructing a new tangent UV frame at every point on the globe.
    let square=wind*wind;
    let weights=square*square+vec3<f32>(0.01);
    let normalized=weights/(weights.x+weights.y+weights.z);
    let coordinate=position/scale+seed+(footprint.gba-0.5)*0.14;
    var fibres=0.0;var weight_sum=0.0;
    for(var axis=0u;axis<3u;axis+=1u) {
        if normalized[axis]<0.04 {continue;}
        var stretch=vec3<f32>(1.80);stretch[axis]=0.11;
        let strand=textureSampleLevel(noise,noise_sampler,coordinate*stretch+f32(axis)*vec3<f32>(0.317,0.619,0.173),0.0);
        fibres+=(strand.b*0.8+strand.a*0.2)*normalized[axis];weight_sum+=normalized[axis];
    }
    fibres/=max(weight_sum,0.001);
    let bank_noise=textureSampleLevel(noise,noise_sampler,base_position/(scale*3.0)+seed+vec3<f32>(0.317,0.137,0.619),0.0).r;
    let threshold=environment_coverage_threshold(coverage,true);
    let cloud_patch=smoothstep(threshold-0.02,threshold+0.04,bank_noise);
    let strands=pow(smoothstep(0.28,0.66,fibres),1.3);
    return cloud_patch*strands*smoothstep(0.0,0.18,height)*(1.0-smoothstep(0.65,1.0,height))*density*0.20;
}
fn environment_density(p:vec3<f32>,parameters:EnvironmentUniform,
    noise:texture_3d<f32>,noise_sampler:sampler,weather_map:texture_cube_array<f32>)->f32 {
    if parameters.weather.w<0.5 || parameters.weather.x<=0.0 || parameters.weather.y<=0.0 {return 0.0;}
    let radius=length(p);let up=p/radius;
    let altitude=(radius-1.0)*parameters.shape.x;
    let minimum_base=environment_cloud_bottom(parameters);
    if altitude<=minimum_base || altitude>=environment_cloud_top(parameters) {return 0.0;}
    let simulated=parameters.simulation.x>0.5;
    var local_parameters=parameters;
    var optical_fields=vec4<f32>(1.0,0.0,0.0,0.0);
    if simulated {
        optical_fields=environment_weather_sample(up,5,parameters,weather_map,noise_sampler);
        local_parameters.shape.y=environment_local_cloud_base(parameters,optical_fields.z);
    }
    if altitude<=local_parameters.shape.y {return 0.0;}
    let relative=(altitude-local_parameters.shape.y)/parameters.shape.z;
    var lower=vec4<f32>(0.0);var upper=vec4<f32>(0.0);
    var optics=optical_fields.xy;var climate=vec4<f32>(0.0,0.0,1.0,0.0);
    var position=environment_rotate(parameters.wind,p)*parameters.shape.x;
    var low_up=normalize(position);
    var high_position=position;
    if simulated {
        if relative<2.40 {lower=environment_weather_sample(up,0,parameters,weather_map,noise_sampler);}
        if relative>1.44 {upper=environment_weather_sample(up,1,parameters,weather_map,noise_sampler);}
        let artistic=parameters.weather.x/0.42;
        lower.w=clamp(lower.w*artistic,0.0,1.0);upper.w=clamp(upper.w*artistic,0.0,1.0);
        if max(lower.w,upper.w)<=0.002 {return 0.0;}
        if lower.w>0.002 && optics.x>0.002 {
            climate=environment_weather_sample(up,2,parameters,weather_map,noise_sampler);
            let flow=environment_cloud_flow(p,up,lower.xyz,parameters);
            position=mix(flow.second,flow.first,flow.blend);low_up=up;
        }
        if upper.w>0.002 && optics.y>0.002 {
            let flow=environment_cloud_flow(p,up,upper.xyz,parameters);
            high_position=mix(flow.second,flow.first,flow.blend);
        }
    } else {lower.w=parameters.weather.x;}
    var density=0.0;
    if lower.w>0.002 && optics.x>0.002 {
        density+=environment_low_cloud(position,low_up,altitude,lower.w,optics.x,climate,local_parameters,noise,noise_sampler);
    }
    if upper.w>0.002 && optics.y>0.002 {
        density+=environment_high_cloud(high_position,up,altitude,upper.w,optics.y,upper.xyz,local_parameters,noise,noise_sampler);
    }
    return clamp(density,0.0,1.0);
}
// Density is a dimensionless optical-thickness multiplier across the layer.
fn environment_extinction(parameters: EnvironmentUniform) -> f32 {
    return parameters.weather.y * 5.0 / max(parameters.shape.z, 0.001);
}
fn environment_sun_visibility(p: vec3<f32>, parameters: EnvironmentUniform) -> f32 {
    let direction = normalize(parameters.sun.xyz);
    let radius=max(length(p),1.0);
    let elevation=dot(p/radius,direction);
    // Elevated clouds/haze see sunset later than the ground. Centre a soft
    // planetary penumbra on that altitude's geometric horizon, rather than
    // switching a whole atmospheric sample from full sunlight to black.
    let horizon=-sqrt(max(1.0-1.0/(radius*radius),0.0));
    return environment_twilight(horizon-0.06,horizon+0.06,elevation);
}
// Signed sunward entry/exit distances through the shell, measured from an
// anchor inside it. The inner positive root selects this planet's near layer.
fn environment_shadow_interval(anchor: vec3<f32>, parameters: EnvironmentUniform) -> vec2<f32> {
    let direction = normalize(parameters.sun.xyz);
    let inner = 1.0 + environment_cloud_bottom(parameters) / parameters.shape.x;
    let outer = 1.0 + environment_cloud_top(parameters) / parameters.shape.x;
    let projection = dot(anchor, direction);
    let square = dot(anchor, anchor);
    return vec2<f32>(-projection + sqrt(max(projection * projection - square + inner * inner, 0.0)),
        -projection + sqrt(max(projection * projection - square + outer * outer, 0.0)));
}
// A sun ray is indexed by its intersection with the middle cloud sphere.
// This is independent of the viewer and applies to every terrain LOD. The
// fourth component clips the optical path for receivers inside the clouds.
fn environment_shadow_coordinate(p: vec3<f32>, parameters: EnvironmentUniform) -> vec4<f32> {
    let direction = normalize(parameters.sun.xyz);
    let elevation = dot(normalize(p), direction);
    if elevation <= 0.015 || (length(p) - 1.0) * parameters.shape.x >= environment_cloud_top(parameters) {
        return vec4<f32>(0.0);
    }
    let middle_radius = 1.0 + ((environment_cloud_bottom(parameters) + environment_cloud_top(parameters)) * 0.5) / parameters.shape.x;
    let projection = dot(p, direction);
    let discriminant = projection * projection - dot(p, p) + middle_radius * middle_radius;
    if discriminant <= 0.0 { return vec4<f32>(0.0); }
    let distance = -projection + sqrt(discriminant);
    let middle = p + direction * distance;
    let interval = environment_shadow_interval(middle, parameters);
    let remaining = clamp((interval.y - max(interval.x, -distance)) / max(interval.y - interval.x, 0.000001), 0.0, 1.0);
    return vec4<f32>(middle / middle_radius, remaining);
}
fn environment_shadow_optical_depth(anchor: vec3<f32>, parameters: EnvironmentUniform,
    noise: texture_3d<f32>, noise_sampler: sampler, weather_map: texture_cube_array<f32>) -> f32 {
    if parameters.weather.w < 0.5 || parameters.weather.x <= 0.0 || parameters.weather.y <= 0.0
        || dot(anchor, parameters.sun.xyz) <= 0.0 { return 0.0; }
    let interval = environment_shadow_interval(anchor, parameters);
    let stride = max(interval.y - interval.x, 0.0) / 12.0;
    let direction = normalize(parameters.sun.xyz);
    var optical_depth = 0.0;
    for (var i = 0u; i < 12u; i += 1u) {
        let sample = anchor + direction * (interval.x + (f32(i) + 0.5) * stride);
        optical_depth += environment_density(sample, parameters, noise, noise_sampler, weather_map);
    }
    return optical_depth * stride * parameters.shape.x * environment_extinction(parameters);
}
fn environment_shadow(p: vec3<f32>, parameters: EnvironmentUniform,
    noise: texture_3d<f32>, noise_sampler: sampler, weather_map: texture_cube_array<f32>) -> f32 {
    if parameters.weather.w < 0.5 || parameters.weather.z <= 0.0 || parameters.weather.x <= 0.0 {
        return 1.0;
    }
    let coordinate = environment_shadow_coordinate(p, parameters);
    if coordinate.w <= 0.0 { return 1.0; }
    let radius = 1.0 + ((environment_cloud_bottom(parameters) + environment_cloud_top(parameters)) * 0.5) / parameters.shape.x;
    let optical_depth = environment_shadow_optical_depth(coordinate.xyz * radius, parameters, noise, noise_sampler, weather_map);
    let transmission = exp(-optical_depth * coordinate.w);
    let elevation = dot(normalize(p), normalize(parameters.sun.xyz));
    let strength = parameters.weather.z * smoothstep(0.015, 0.13, elevation);
    return mix(1.0, transmission, strength);
}
fn environment_cube_direction(face: u32, uv: vec2<f32>) -> vec3<f32> {
    let p = uv * 2.0 - 1.0;
    switch face {
        case 0u: { return normalize(vec3<f32>(1.0, -p.y, -p.x)); }
        case 1u: { return normalize(vec3<f32>(-1.0, -p.y, p.x)); }
        case 2u: { return normalize(vec3<f32>(p.x, 1.0, p.y)); }
        case 3u: { return normalize(vec3<f32>(p.x, -1.0, -p.y)); }
        case 4u: { return normalize(vec3<f32>(p.x, -p.y, 1.0)); }
        default: { return normalize(vec3<f32>(-p.x, -p.y, -1.0)); }
    }
}
// Exposed by the surface integration as an approximate direct-light factor.
// A floor preserves indirect skylight beneath dense cloud shadows.
fn environment_surface_light(p: vec3<f32>, parameters: EnvironmentUniform,
    noise: texture_3d<f32>, noise_sampler: sampler, weather_map: texture_cube_array<f32>) -> f32 {
    return mix(0.35, 1.0, environment_shadow(p, parameters, noise, noise_sampler, weather_map));
}
fn environment_cloud_light(p: vec3<f32>, view_direction: vec3<f32>, parameters: EnvironmentUniform,
    noise: texture_3d<f32>, noise_sampler: sampler, weather_map: texture_cube_array<f32>) -> vec3<f32> {
    let sun = normalize(parameters.sun.xyz);
    let daylight = environment_sun_visibility(p, parameters);
    // Growing sunward segments reach across deep cloud bodies. This resolves
    // lit rims and darker interiors instead of uniformly lighting a thin roof.
    let fractions=array<f32,4>(0.12,0.24,0.44,0.72);
    var optical=0.0;var travelled=0.0;var first_density=0.0;
    for(var i=0u;i<4u;i+=1u) {
        let span=parameters.shape.z*fractions[i];
        let sample=p+sun*((travelled+span*0.5)/parameters.shape.x);
        let density=environment_density(sample,parameters,noise,noise_sampler,weather_map);
        if i==0u {first_density=density;}
        optical+=density*span;travelled+=span;
    }
    optical*=environment_extinction(parameters);
    let direct_transmission=exp(-optical);
    let transmission=direct_transmission+0.12*(1.0-direct_transmission)*exp(-optical*0.25);
    let cosine = clamp(dot(view_direction, sun), -1.0, 1.0);
    let phase = 0.6975 / pow(max(1.3025 - 1.1 * cosine, 0.08), 1.5);
    let altitude_light = smoothstep(-0.15, 0.15, dot(normalize(p), sun));
    // Clouds see a lower horizon than the ground. Their sunset colour follows
    // that same raised horizon as the soft planetary shadow, including orbit.
    let radius=max(length(p),1.0);
    let horizon=-sqrt(max(1.0-1.0/(radius*radius),0.0));
    let sunset_progress=1.0-environment_twilight(horizon-0.06,0.15,dot(normalize(p),sun));
    let tint=environment_sunset_palette(sunset_progress);
    let direct_colour = parameters.light.rgb;
    var lighting_base=parameters.shape.y;
    if parameters.simulation.x>0.5 {
        let terrain=environment_weather_sample(normalize(p),5,parameters,weather_map,noise_sampler).z;
        lighting_base=environment_local_cloud_base(parameters,terrain);
    }
    let height=(length(p)-1.0)*parameters.shape.x-lighting_base;
    let skylight=mix(0.45,1.0,smoothstep(0.0,parameters.shape.z*1.4,height));
    let ambient_visibility=0.35+0.65*exp(-first_density*2.5);
    let ambient = vec3<f32>(0.34, 0.48, 0.70) * parameters.sun.w * 0.006 * altitude_light*skylight*ambient_visibility;
    let direct = direct_colour * parameters.sun.w * (0.035 + phase * 0.022) * transmission * daylight;
    return (ambient + direct)*tint + vec3<f32>(0.12, 0.17, 0.28);
}
fn environment_clear_sky(p: vec3<f32>, direction: vec3<f32>, parameters: EnvironmentUniform) -> vec3<f32> {
    if parameters.light.w < 0.5 { return vec3<f32>(0.0); }
    let up = normalize(p);
    let elevation = clamp(dot(up, direction), 0.0, 1.0);
    let sun_height = dot(up, normalize(parameters.sun.xyz));
    let day = smoothstep(-0.16, 0.12, sun_height);
    let horizon = vec3<f32>(0.43, 0.61, 0.88);
    let zenith = vec3<f32>(0.09, 0.27, 0.60);
    let sky = mix(horizon, zenith, pow(elevation, 0.35));
    let sunset = exp(-abs(sun_height) * 8.0) * pow(max(dot(direction, parameters.sun.xyz), 0.0), 4.0);
    return (sky * day * environment_surface_tint(p,parameters) + vec3<f32>(1.0, 0.22, 0.05) * sunset * 0.32) * parameters.sun.w * 0.018
        + vec3<f32>(0.08, 0.12, 0.22);
}
