#define_import_path planet_bevy::water
#import planet_bevy::environment_material::{material_cloud_shadow,material_sky}
#import bevy_pbr::{mesh_view_bindings::{view,lights,view_transmission_texture},shadows::fetch_directional_shadow,
 transmission::fetch_transmissive_background_non_rough}
#ifdef DEPTH_PREPASS
#import bevy_pbr::mesh_view_bindings::depth_prepass_texture
#endif
struct WaterUniform {shape:vec4<f32>,optics:vec4<f32>,absorption:vec4<f32>,river:vec4<f32>,effects:vec4<f32>}
struct ScenePoint {position:vec3<f32>,eye:f32,valid:f32}
// Raster positions and scene textures cover the whole render target. The
// projection matrix and reflection rays use coordinates inside this camera's
// viewport, which starts to the right of the parameter panel when it is open.
fn view_uv(pixel:vec2<f32>)->vec2<f32> {return (pixel-view.viewport.xy)/view.viewport.zw;}
fn view_pixel(uv:vec2<f32>)->vec2<f32> {return uv*view.viewport.zw+view.viewport.xy;}
fn view_texture_uv(uv:vec2<f32>,size:vec2<f32>)->vec2<f32> {return view_pixel(uv)/size;}
fn viewport_fade(uv:vec2<f32>)->f32 {return smoothstep(0.0,0.035,min(min(uv.x,uv.y),min(1.0-uv.x,1.0-uv.y)));}
fn scene_point(uv:vec2<f32>,radius:f32)->ScenePoint {
#ifdef DEPTH_PREPASS
 let size=vec2<i32>(textureDimensions(depth_prepass_texture));
 let minimum=vec2<i32>(view.viewport.xy);
 let maximum=min(minimum+vec2<i32>(view.viewport.zw),size)-1;
 let pixel=clamp(vec2<i32>(view_pixel(uv)),minimum,maximum);
 let raw=textureLoad(depth_prepass_texture,pixel,0);
 let centre=view_uv(vec2<f32>(pixel)+0.5);
 let clip=vec4<f32>(centre*vec2<f32>(2.0,-2.0)+vec2<f32>(-1.0,1.0),max(raw,0.00000001),1.0);
 let world=view.world_from_clip*clip;let p=world.xyz/world.w;
 return ScenePoint(p,-(view.view_from_world*vec4<f32>(p,1.0)).z*radius,select(0.0,1.0,raw>0.0));
#else
 return ScenePoint(view.world_position,1000.0,0.0);
#endif
}
fn scene_colour(uv:vec2<f32>)->vec3<f32> {
 // Bevy reverses in-shader tone mapping here when the main target is LDR.
 let colour_uv=view_texture_uv(clamp(uv,vec2<f32>(0.001),vec2<f32>(0.999)),
     vec2<f32>(textureDimensions(view_transmission_texture)));
 return fetch_transmissive_background_non_rough(colour_uv,vec3<f32>(0.0,0.0,1.0)).rgb;
}
fn water_fresnel(cosine:f32)->f32 {return 0.02037+0.97963*pow(1.0-clamp(cosine,0.0,1.0),5.0);}
fn water_geometry_shadow(p:vec3<f32>,normal:vec3<f32>,pixel:vec2<f32>)->f32 {
 if (lights.directional_lights[0].flags&1u)==0u {return 1.0;}
 let z=(view.view_from_world*vec4<f32>(p,1.0)).z;
 return fetch_directional_shadow(0u,vec4<f32>(p,1.0),normal,z,pixel);
}
fn water_shadow(p:vec3<f32>,normal:vec3<f32>,pixel:vec2<f32>)->f32 {
 return water_geometry_shadow(p,normal,pixel)*material_cloud_shadow(p);
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
fn reflected_scene(p:vec3<f32>,normal:vec3<f32>,geometric:vec3<f32>,v:vec3<f32>,radius:f32)->ReflectionHit {
 let miss=ReflectionHit(vec2<f32>(0.0),0.0);
 let direction=reflect(-v,normal);
 // A perturbed normal must not turn the reflection into a view of the bed.
 if dot(direction,geometric)<=0.01 {return miss;}
 var distance=0.08;var previous=0.0;var in_front=false;
 for(var i=0u;i<28u;i+=1u) {
  let point=p+(geometric*0.04+direction*distance)/radius;
  let clip=view.clip_from_world*vec4<f32>(point,1.0);
  if clip.w<=0.0 {break;}
  let uv=clip.xy/clip.w*vec2<f32>(0.5,-0.5)+0.5;
  if viewport_fade(uv)<=0.0 {break;}
  let sample=scene_point(uv,radius);
  let eye=-(view.view_from_world*vec4<f32>(point,1.0)).z*radius;
  let delta=eye-sample.eye;
  if sample.valid>0.0 && delta>0.0 {
   // Only refine a front-to-back crossing. Starting behind an occluder is
   // not a reflection hit, even when it is close to the water's silhouette.
   if !in_front {return miss;}
   var near=previous;var far=distance;var hit_uv=uv;
   for(var j=0u;j<8u;j+=1u) {
    let middle=(near+far)*0.5;
    let probe=p+(geometric*0.04+direction*middle)/radius;
    let q=view.clip_from_world*vec4<f32>(probe,1.0);
    if q.w<=0.0 {return miss;}
    let coords=q.xy/q.w*vec2<f32>(0.5,-0.5)+0.5;
    let depth=scene_point(coords,radius);
    let z=-(view.view_from_world*vec4<f32>(probe,1.0)).z*radius;
    if depth.valid>0.0 && z>depth.eye {far=middle;hit_uv=coords;} else {near=middle;}
   }
   let hit=scene_point(hit_uv,radius);
   let ray_point=p+(geometric*0.04+direction*far)/radius;
   let error=length(hit.position-ray_point)*radius;
   let pixel_metres=2.0*hit.eye*max(1.0/(abs(view.clip_from_view[0][0])*view.viewport.z),
       1.0/(abs(view.clip_from_view[1][1])*view.viewport.w));
   let thickness=clamp(pixel_metres*1.5,0.03,0.20);
   // A depth discontinuity can bracket an edge without intersecting its
   // surface. Reject that edge instead of smearing foreground colours.
   if hit.valid<=0.0 || dot(hit.position-p,geometric)*radius<0.0 || error>=thickness {return miss;}
   return ReflectionHit(hit_uv,viewport_fade(hit_uv)*(1.0-smoothstep(thickness*0.5,thickness,error))
       *(1.0-smoothstep(70.0,120.0,far)));
  }
  in_front=sample.valid<=0.0 || delta<=0.0;
  previous=distance;distance=distance*1.28+0.12;
  if distance>120.0 {break;}
 }
 return miss;
}

// Keep distortion in physical space: zooming out must not move a shallow
// river's refraction metres sideways across dry banks.
fn bounded_refraction_offset(offset:vec2<f32>,eye:f32,path:f32)->vec2<f32> {
 let projection=vec2<f32>(abs(view.clip_from_view[0][0]),abs(view.clip_from_view[1][1]));
 let metres=length(offset*2.0*eye/max(projection,vec2<f32>(0.0001)));
 let limit=min(max(path,0.0)*0.4,0.75);
 return offset*min(1.0,limit/max(metres,0.000001));
}
// A downstream bank can be farther from the camera while still above water.
fn behind_water(point:ScenePoint,p:vec3<f32>,geometric:vec3<f32>,eye:f32,radius:f32)->bool {
 return point.valid<=0.0 || (point.eye>eye+0.001 && dot(point.position-p,geometric)*radius<=0.002);
}
struct WaterOptics {colour:vec3<f32>,path:f32,scene:vec3<f32>}
fn shade_water(p:vec3<f32>,pixel:vec2<f32>,normal:vec3<f32>,geometric:vec3<f32>,up:vec3<f32>,v:vec3<f32>,ripple:vec2<f32>,
 lit_body:vec3<f32>,roughness:f32,settings:WaterUniform,sea:bool,underside:bool)->WaterOptics {
 let radius=settings.shape.x;
 let uv=view_uv(pixel);
 let surface_eye=-(view.view_from_world*vec4<f32>(p,1.0)).z*radius;
 let original=scene_point(uv,radius);
 let original_depth=select(1000.0,max(original.eye-surface_eye,0.0),original.valid>0.0);
 let view_normal=(view.view_from_world*vec4<f32>(normal,0.0)).xy;
 let depth_weight=smoothstep(0.02,max(settings.shape.w,0.021),original_depth);
 let view_v=(view.view_from_world*vec4<f32>(v,0.0)).xyz;
 let original_path=original_depth/max(abs(view_v.z),0.05);
 let desired=(ripple+view_normal*0.22)*settings.optics.z*settings.shape.z*depth_weight
     *mix(0.25,1.0,max(dot(normal,v),0.0))*viewport_fade(uv);
 let offset=bounded_refraction_offset(desired,surface_eye,original_path);
 let candidate=scene_point(uv+offset,radius);
 let valid=viewport_fade(uv+offset)*select(0.0,1.0,behind_water(candidate,p,geometric,surface_eye,radius))
     *select(1.0,smoothstep(0.0,0.15,candidate.eye-surface_eye),candidate.valid>0.0);
 var final_uv=uv+offset*valid;
 var point=scene_point(final_uv,radius);
 if !behind_water(point,p,geometric,surface_eye,radius) {final_uv=uv;point=original;}
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
 let sky=material_sky(reflected)*view.exposure;
 var reflection=sky;
 let reflection_distance=length(view.world_position-p)*radius;
 if settings.optics.w>0.0 && !underside &&reflection_distance<160.0 {
  let hit=reflected_scene(p,normal,geometric,v,radius);
  if hit.weight>0.001 {
   let spread=roughness*roughness*0.008;
   // The traced normal already includes ripples. Moving the lookup again
   // samples an unrelated surface, especially along shore silhouettes.
   let reflection_uv=hit.uv;
   let blurred=(scene_colour(reflection_uv+vec2<f32>(spread,0.0))+scene_colour(reflection_uv-vec2<f32>(spread,0.0))
       +scene_colour(reflection_uv+vec2<f32>(0.0,spread))+scene_colour(reflection_uv-vec2<f32>(0.0,spread)))*0.25;
   reflection=mix(sky,blurred,hit.weight*settings.optics.w*(1.0-smoothstep(100.0,160.0,reflection_distance)));
  }
 }
 let geometry_shadow=water_geometry_shadow(p,normal,pixel);
 let shadow=geometry_shadow*material_cloud_shadow(p);
 // The reflected cube already contains the lit cloud volume. Cloud shadows
 // attenuate direct glints, not that incoming sky radiance a second time.
 reflection*=mix(0.3,1.0,geometry_shadow);
 if underside {
  let transmitted_squared=1.333*1.333*(1.0-pow(max(dot(normal,v),0.0),2.0));
  fresnel=mix(fresnel,1.0,smoothstep(0.97,1.0,transmitted_squared));
  volume=refracted;reflection=lit_body;
 }
 let glint=lights.directional_lights[0].color.rgb*view.exposure*sun_glint(normal,v,roughness)*settings.optics.y*shadow;
 return WaterOptics(mix(volume,reflection,clamp(fresnel*settings.optics.x,0.0,1.0))+glint,path,point.position);
}
