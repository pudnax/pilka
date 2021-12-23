use std::fs::File;
use std::io::Write;
use std::path::Path;

use super::utils::create_folder;

mod glsl;
mod wgsl;

pub fn create_default_shaders<P: AsRef<Path>>(
    name: P,
    wgsl_mode: Option<()>,
) -> std::io::Result<()> {
    create_folder(&name)?;

    let create_file = |filename: &str, content: &str| -> std::io::Result<()> {
        let path = name.as_ref().join(filename);
        let mut file = File::create(path)?;
        file.write_all(content.as_bytes())
    };

    if wgsl_mode.is_some() {
        create_file("prelude.glsl", glsl::PRELUDE)?;
        create_file("shader.frag.wgsl", wgsl::FRAG_SHADER)?;
        create_file("shader.vert.wgsl", wgsl::VERT_SHADER)?;
        create_file("shader.comp.wgsl", wgsl::COMP_SHADER)?;
    } else {
        create_file("prelude.glsl", glsl::PRELUDE)?;
        create_file("shader.frag", glsl::FRAG_SHADER)?;
        create_file("shader.vert", glsl::VERT_SHADER)?;
        create_file("shader.comp", glsl::COMP_SHADER)?;
    }

    Ok(())
}
