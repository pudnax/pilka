#![allow(clippy::new_without_default)]

use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString},
    hash::Hash,
    ops::{Deref, DerefMut},
    path::PathBuf,
};

pub fn dispatch_optimal_size(len: u32, subgroup_size: u32) -> u32 {
    let padded_size = (subgroup_size - len % subgroup_size) % subgroup_size;
    (len + padded_size) / subgroup_size
}

pub type Frame = (Vec<u8>, ImageDimentions);

pub enum ShaderData {
    Render { vert: Vec<u32>, frag: Vec<u32> },
    Compute(Vec<u32>),
}

#[derive(Debug, Clone)]
pub struct ShaderCreateInfo<'a> {
    pub data: &'a [u32],
    pub entry_point: &'a CStr,
}

impl<'a> ShaderCreateInfo<'a> {
    pub fn new(data: &'a [u32], entry_point: &'a CStr) -> Self {
        Self { data, entry_point }
    }
}

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

#[derive(Debug, Clone, Copy)]
pub struct ImageDimentions {
    pub width: u32,
    pub height: u32,
    pub unpadded_bytes_per_row: u32,
    pub padded_bytes_per_row: u32,
}

impl ImageDimentions {
    pub fn new(width: u32, height: u32, align: u32) -> Self {
        let bytes_per_pixel = std::mem::size_of::<[u8; 4]>() as u32;
        let unpadded_bytes_per_row = width * bytes_per_pixel;
        let row_padding = (align - unpadded_bytes_per_row % align) % align;
        let padded_bytes_per_row = unpadded_bytes_per_row + row_padding;
        Self {
            width,
            height,
            unpadded_bytes_per_row,
            padded_bytes_per_row,
        }
    }

    pub fn linear_size(&self) -> u64 {
        self.padded_bytes_per_row as u64 * self.height as u64
    }
}

pub struct ContiniousHashMap<K, V>(HashMap<K, HashSet<V>>);

impl<K, V> Deref for ContiniousHashMap<K, V> {
    type Target = HashMap<K, HashSet<V>>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<K, V> DerefMut for ContiniousHashMap<K, V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<K, V> ContiniousHashMap<K, V> {
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

impl<K: Eq + Hash, V: Eq + Hash> ContiniousHashMap<K, V> {
    pub fn push_value(&mut self, key: K, value: V) {
        self.0.entry(key).or_insert_with(HashSet::new).insert(value);
    }
}
