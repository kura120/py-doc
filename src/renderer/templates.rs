use rust_embed::RustEmbed;

#[derive(RustEmbed)]
#[folder = "src/render/assets"]
pub struct Asset;