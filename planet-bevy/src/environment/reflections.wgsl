#import planet_bevy::environment_air::environment_air
#import planet_bevy::environment::{EnvironmentUniform,environment_density,environment_cloud_light,environment_clear_sky,environment_extinction}
struct ReflectionStep { environment:EnvironmentUniform, face:vec4<u32> }
@group(0) @binding(0) var noise:texture_3d<f32>;
@group(0) @binding(1) var noise_sampler:sampler;
@group(0) @binding(2) var result:texture_storage_2d_array<rgba16float,write>;
@group(0) @binding(3) var<uniform> step:ReflectionStep;
fn cube_direction(face:u32,uv:vec2<f32>)->vec3<f32> {
    let p=uv*2.0-1.0;
    switch face {
        case 0u:{return normalize(vec3<f32>(1.0,-p.y,-p.x));}
        case 1u:{return normalize(vec3<f32>(-1.0,-p.y,p.x));}
        case 2u:{return normalize(vec3<f32>(p.x,1.0,p.y));}
        case 3u:{return normalize(vec3<f32>(p.x,-1.0,-p.y));}
        case 4u:{return normalize(vec3<f32>(p.x,-p.y,1.0));}
        default:{return normalize(vec3<f32>(-p.x,-p.y,-1.0));}
    }
}
@compute @workgroup_size(8,8,1)
fn update(@builtin(global_invocation_id) id:vec3<u32>) {
    let size=textureDimensions(result);
    if any(id.xy>=size) {return;}
    let face=(step.face.x+id.z)%6u;
    let direction=cube_direction(face,(vec2<f32>(id.xy)+0.5)/vec2<f32>(size));
    let parameters=step.environment;
    let origin=normalize(parameters.viewer.xyz+vec3<f32>(0.0,0.0000001,0.0))*(1.0+1.7/max(parameters.shape.x,1.0));
    var colour=environment_clear_sky(origin,direction,parameters);
    if parameters.weather.w>0.5 && parameters.weather.x>0.0 && parameters.weather.y>0.0 {
        let inner=1.0+parameters.shape.y/parameters.shape.x;
        let outer=inner+parameters.shape.z/parameters.shape.x;
        let b=dot(origin,direction);
        let discriminant=b*b-dot(origin,origin)+outer*outer;
        if discriminant>0.0 {
            var near=max(-b-sqrt(discriminant),0.0);
            var far=-b+sqrt(discriminant);
            let inner_discriminant=b*b-dot(origin,origin)+inner*inner;
            if inner_discriminant>0.0 {near=max(near,-b+sqrt(inner_discriminant));}
            let ground_discriminant=b*b-dot(origin,origin)+1.0;
            if ground_discriminant>0.0 {
                let ground=-b-sqrt(ground_discriminant);
                if ground>0.0 {far=min(far,ground);}
            }
            if far>near {
                let stride=(far-near)/20.0;
                let air=environment_air(origin,origin+direction*((near+far)*0.5),parameters);
                var transmission=1.0;
                var scattering=vec3<f32>(0.0);
                for(var i=0u;i<20u;i+=1u) {
                    let p=origin+direction*(near+(f32(i)+0.5)*stride);
                    let density=environment_density(p,parameters,noise,noise_sampler);
                    if density>0.001 {
                        let opacity=1.0-exp(-density*stride*parameters.shape.x*environment_extinction(parameters));
                        scattering+=transmission*opacity*(environment_cloud_light(p,direction,parameters,noise,noise_sampler)*air.transmission+air.inscattering);
                        transmission*=1.0-opacity;
                        if transmission<0.015 {break;}
                    }
                }
                colour=scattering+colour*transmission;
            }
        }
    }
    let column_point=direction*(1.0+(parameters.shape.y+parameters.shape.z*0.45)/parameters.shape.x);
    let column=environment_density(column_point,parameters,noise,noise_sampler)*0.85;
    textureStore(result,vec2<i32>(id.xy),i32(face),vec4<f32>(max(colour,vec3<f32>(0.0)),column));
}
