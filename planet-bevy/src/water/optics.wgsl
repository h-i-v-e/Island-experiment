#define_import_path planet_bevy::water
#import bevy_pbr::{mesh_view_bindings::{view,lights},shadows::fetch_directional_shadow,
 transmission::fetch_transmissive_background_non_rough}
#ifdef DEPTH_PREPASS
#import bevy_pbr::mesh_view_bindings::depth_prepass_texture
#endif
struct WaterUniform {shape:vec4<f32>,optics:vec4<f32>,absorption:vec4<f32>,river:vec4<f32>,effects:vec4<f32>}
struct ScenePoint {position:vec3<f32>,eye:f32,valid:f32}
fn viewport_fade(uv:vec2<f32>)->f32 {return smoothstep(0.0,0.035,min(min(uv.x,uv.y),min(1.0-uv.x,1.0-uv.y)));}
fn scene_point(uv:vec2<f32>,radius:f32)->ScenePoint {
#ifdef DEPTH_PREPASS
 let size=vec2<i32>(textureDimensions(depth_prepass_texture));
 let pixel=clamp(vec2<i32>(uv*vec2<f32>(size)),vec2<i32>(0),size-1);
 let raw=textureLoad(depth_prepass_texture,pixel,0);
 let centre=(vec2<f32>(pixel)+0.5)/vec2<f32>(size);
 let clip=vec4<f32>(centre*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),max(raw,0.00000001),1.0);
 let world=view.world_from_clip*clip;let p=world.xyz/world.w;
 return ScenePoint(p,-(view.view_from_world*vec4<f32>(p,1.0)).z*radius,select(0.0,1.0,raw>0.0));
#else
 return ScenePoint(view.world_position,1000.0,0.0);
#endif
}
fn scene_colour(uv:vec2<f32>)->vec3<f32> {
 // Bevy reverses in-shader tone mapping here when the main target is LDR.
 return fetch_transmissive_background_non_rough(clamp(uv,vec2<f32>(0.001),vec2<f32>(0.999)),vec3<f32>(0.0,0.0,1.0)).rgb;
}
fn water_fresnel(cosine:f32)->f32 {return 0.02037+0.97963*pow(1.0-clamp(cosine,0.0,1.0),5.0);}
fn water_shadow(p:vec3<f32>,normal:vec3<f32>,pixel:vec2<f32>)->f32 {
 if (lights.directional_lights[0].flags&1u)==0u {return 1.0;}
 let z=(view.view_from_world*vec4<f32>(p,1.0)).z;
 return fetch_directional_shadow(0u,vec4<f32>(p,1.0),normal,z,pixel);
}
fn sun_glint(normal:vec3<f32>,v:vec3<f32>,roughness:f32)->f32 {
 let l=lights.directional_lights[0].direction_to_light;
 let nv=max(dot(normal,v),0.001);let nl=max(dot(normal,l),0.0);
 let h=normalize(v+l+vec3<f32>(0.0,0.000001,0.0));
 let nh=max(dot(normal,h),0.0);let vh=max(dot(v,h),0.0);
 let a=max(roughness*roughness,0.006);let a2=a*a;
 let d=nh*nh*(a2-1.0)+1.0;
 let distribution=a2/max(3.14159265*d*d,0.0000000001);
 let visibility=0.5/max(nl*sqrt(nv*nv*(1.0-a2)+a2)+nv*sqrt(nl*nl*(1.0-a2)+a2),0.0001);
 return distribution*visibility*water_fresnel(vh)*nl;
}
struct ReflectionHit {uv:vec2<f32>,weight:f32}
fn reflected_scene(p:vec3<f32>,normal:vec3<f32>,v:vec3<f32>,radius:f32)->ReflectionHit {
 let direction=reflect(-v,normal);
 var distance=0.08;var previous=0.0;
 for(var i=0u;i<28u;i+=1u) {
  let point=p+(normal*0.04+direction*distance)/radius;
  let clip=view.clip_from_world*vec4<f32>(point,1.0);
  if clip.w<=0.0 {break;}
  let uv=clip.xy/clip.w*vec2<f32>(0.5,-0.5)+0.5;
  if viewport_fade(uv)<=0.0 {break;}
  let sample=scene_point(uv,radius);
  let eye=-(view.view_from_world*vec4<f32>(point,1.0)).z*radius;
  let delta=eye-sample.eye;
  if sample.valid>0.0 && delta>0.0 && delta<max(0.15,(distance-previous)*2.0) {
   var near=previous;var far=distance;var hit_uv=uv;
   for(var j=0u;j<5u;j+=1u) {
    let middle=(near+far)*0.5;
    let probe=p+(normal*0.04+direction*middle)/radius;
    let q=view.clip_from_world*vec4<f32>(probe,1.0);
    let coords=q.xy/q.w*vec2<f32>(0.5,-0.5)+0.5;
    let depth=scene_point(coords,radius);
    let z=-(view.view_from_world*vec4<f32>(probe,1.0)).z*radius;
    if depth.valid>0.0 && z>depth.eye {far=middle;hit_uv=coords;} else {near=middle;}
   }
   let hit=scene_point(hit_uv,radius);
   let ray_point=p+(normal*0.04+direction*far)/radius;
   let error=length(hit.position-ray_point)*radius;
   return ReflectionHit(hit_uv,viewport_fade(hit_uv)*(1.0-smoothstep(0.1,1.5,error))*(1.0-smoothstep(70.0,120.0,far)));
  }
  previous=distance;distance=distance*1.28+0.12;
  if distance>120.0 {break;}
 }
 return ReflectionHit(vec2<f32>(0.0),0.0);
}
struct WaterOptics {colour:vec3<f32>,path:f32,scene:vec3<f32>}
fn shade_water(p:vec3<f32>,pixel:vec2<f32>,normal:vec3<f32>,up:vec3<f32>,v:vec3<f32>,ripple:vec2<f32>,
 lit_body:vec3<f32>,roughness:f32,settings:WaterUniform,sea:bool,underside:bool)->WaterOptics {
 let radius=settings.shape.x;
 let uv=pixel/view.viewport.zw;
 let surface_eye=-(view.view_from_world*vec4<f32>(p,1.0)).z*radius;
 let original=scene_point(uv,radius);
 let original_depth=select(1000.0,max(original.eye-surface_eye,0.0),original.valid>0.0);
 let view_normal=(view.view_from_world*vec4<f32>(normal,0.0)).xy;
 let depth_weight=smoothstep(0.02,max(settings.shape.w,0.021),original_depth);
 let offset=(ripple*settings.optics.z+view_normal*0.22)*settings.shape.z*depth_weight
     *mix(0.25,1.0,max(dot(normal,v),0.0))*viewport_fade(uv);
 let candidate=scene_point(uv+offset,radius);
 let valid=viewport_fade(uv+offset)*select(1.0,smoothstep(0.0,0.15,candidate.eye-surface_eye),candidate.valid>0.0);
 var final_uv=uv+offset*valid;
 var point=scene_point(final_uv,radius);
 if point.valid>0.0 && point.eye<=surface_eye+0.001 {final_uv=uv;point=original;}
 let view_v=(view.view_from_world*vec4<f32>(v,0.0)).xyz;
 var path=min(1000.0,max(point.eye-surface_eye,0.0)/max(abs(view_v.z),0.05));
 if point.valid==0.0 {path=1000.0;}
 var refracted=scene_colour(final_uv);
 if sea {
  let visibility=1.0-smoothstep(6.0,9.5,max((1.0-length(point.position))*radius,0.0));
  refracted=mix(lit_body,refracted,visibility*point.valid);
 }
 let transmission=exp(-max(settings.absorption.rgb,vec3<f32>(0.0))*settings.absorption.a*path);
 var volume=mix(lit_body,refracted,transmission);
 var fresnel=water_fresnel(dot(normal,v));
 let reflected=reflect(-v,normal);
 let daylight=0.03+0.97*smoothstep(-0.08,0.2,dot(up,lights.directional_lights[0].direction_to_light));
 let sky=mix(vec3<f32>(0.420,0.587,0.749),vec3<f32>(0.205,0.420,0.638),clamp(dot(reflected,up),0.0,1.0))*daylight;
 var reflection=sky;
 let reflection_distance=length(view.world_position-p)*radius;
 if settings.optics.w>0.0 && !underside &&reflection_distance<160.0 {
  let hit=reflected_scene(p,normal,v,radius);
  if hit.weight>0.001 {
   let spread=roughness*roughness*0.008;
   let reflection_uv=hit.uv+ripple*settings.optics.z*select(0.006,0.008,sea)*viewport_fade(hit.uv);
   let blurred=(scene_colour(reflection_uv+vec2<f32>(spread,0.0))+scene_colour(reflection_uv-vec2<f32>(spread,0.0))
       +scene_colour(reflection_uv+vec2<f32>(0.0,spread))+scene_colour(reflection_uv-vec2<f32>(0.0,spread)))*0.25;
   reflection=mix(sky,blurred,hit.weight*settings.optics.w*(1.0-smoothstep(100.0,160.0,reflection_distance)));
  }
 }
 let shadow=water_shadow(p,normal,pixel);
 reflection*=mix(0.3,1.0,shadow);
 if underside {
  let transmitted_squared=1.333*1.333*(1.0-pow(max(dot(normal,v),0.0),2.0));
  fresnel=mix(fresnel,1.0,smoothstep(0.97,1.0,transmitted_squared));
  volume=refracted;reflection=lit_body;
 }
 let glint=lights.directional_lights[0].color.rgb*view.exposure*sun_glint(normal,v,roughness)*settings.optics.y*shadow;
 return WaterOptics(mix(volume,reflection,clamp(fresnel*settings.optics.x,0.0,1.0))+glint,path,point.position);
}
