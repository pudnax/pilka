use std::{ffi::CString, path::PathBuf};

pub type Frame<'a> = (&'a [u8], ImageDimentions);

#[derive(Debug, Clone)]
pub enum PipelineInfo {
    Rendering { vert: ShaderInfo, frag: ShaderInfo },
    Compute { comp: ShaderInfo },
}

#[derive(Hash, Debug, Clone)]
pub struct ShaderInfo {
    pub path: PathBuf,
    pub entry_point: CString,
}

impl ShaderInfo {
    pub fn new(path: PathBuf, entry_point: String) -> ShaderInfo {
        ShaderInfo {
            path,
            entry_point: CString::new(entry_point).unwrap(),
        }
    }
}

#[derive(Hash, Debug, Clone)]
pub enum ShaderType {
    Glsl,
    Wgsl,
    Spir,
}

#[derive(Hash, Debug, Clone)]
pub enum ShaderStage {
    Vertex = 0,
    Fragment,
    Compute,
}

#[derive(Hash, Debug, Clone)]
pub struct ShaderData {
    pub source: String,
    pub entry_point: String,
    pub ty: ShaderType,
    pub stage: ShaderStage,
}

#[derive(Debug, Clone, Copy)]
pub struct ImageDimentions {
    pub width: usize,
    pub height: usize,
    pub padded_bytes_per_row: usize,
    pub unpadded_bytes_per_row: usize,
}

impl ImageDimentions {
    pub fn new(width: usize, height: usize, align: usize) -> Self {
        let bytes_per_pixel = std::mem::size_of::<[u8; 4]>();
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let padded_bytes_per_row_padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + padded_bytes_per_row_padding;
        Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
        }
    }
}
