// All island components consume one resolved role set, including the satellite.
impl Renderer{
 fn prepare_dynamic(&mut self,s:&Scene<'_>){
  let mut resolved=dotisle_domain::dynamic_color::resolve(dotisle_domain::dynamic_color::Source::parse(s.dynamic_source),self.cover_seed,s.dynamic_system_seed,s.dynamic_custom_seed,true,s.dynamic_high_contrast);
  self.color_motion.update(resolved.roles,s.time_ms/1000.,s.animate);resolved.roles=self.color_motion.sample(s.time_ms/1000.);let c=resolved.roles.ui();
  // The body tint must actually read as the source colour. surface_container_high alone is
  // nearly the same neutral as the fixed fallback, which made "dynamic colour on" look off.
  // Blending toward primary_container keeps the surface dark while carrying the source hue.
  let body=crate::cover_color::mix(resolved.roles.surface_container_high,resolved.roles.primary_container,0.55);
  self.paint_palette=crate::cover_color::CoverPalette{background:body,accent:resolved.roles.primary};self.m3_cache=Some((resolved.seed,true,c));self.dynamic_resolved=Some(resolved);
 }
}
