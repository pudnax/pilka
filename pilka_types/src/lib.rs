#![allow(clippy::new_without_default)]

use bytemuck::{Pod, Zeroable};

use std::{
    collections::{HashMap, HashSet},
    ffi::{CStr, CString},
    hash::Hash,
    ops::{Deref, DerefMut},
    path::PathBuf,
    time::Duration,
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

#[derive(Hash, Debug, Clone, Copy)]
pub enum ShaderFlavour {
    Glsl,
    Wgsl,
}

#[derive(Hash, Debug, Clone)]
pub struct ShaderInfo {
    pub path: PathBuf,
    pub entry_point: CString,
    pub flavour: ShaderFlavour,
}

impl ShaderInfo {
    pub fn new(path: PathBuf, entry_point: String, flavour: ShaderFlavour) -> ShaderInfo {
        ShaderInfo {
            path,
            entry_point: CString::new(entry_point).unwrap(),
            flavour,
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

/// A hash map with a [HashSet](std::collections::HashSet) to hold unique values
#[derive(Debug)]
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
    /// Creates an empty [ContiniousHashMap]
    ///
    /// The hash map is initially created with a capacity of 0,
    /// so it will not allocate until it is first inserted into.
    pub fn new() -> Self {
        Self(HashMap::new())
    }
}

impl<K: Eq + Hash, V: Eq + Hash> ContiniousHashMap<K, V> {
    /// Inserts a key-value pair into the map.
    ///
    /// If the mep already contain this key this method will add
    /// a value instead of rewriting an old value.
    pub fn push_value(&mut self, key: K, value: V) {
        self.0.entry(key).or_insert_with(HashSet::new).insert(value);
    }
}

#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct PushConstant {
    pub pos: [f32; 3],
    pub time: f32,
    pub wh: [f32; 2],
    pub mouse: [f32; 2],
    pub mouse_pressed: u32,
    pub frame: u32,
    pub time_delta: f32,
    pub record_period: f32,
}

impl PushConstant {
    pub fn as_slice(&self) -> &[u8] {
        unsafe { any_as_u8_slice(self) }
    }

    pub fn size() -> u32 {
        std::mem::size_of::<Self>() as _
    }
}

/// # Safety
/// Until you're using it on not ZST or DST it's fine
pub unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    std::slice::from_raw_parts((p as *const T) as *const _, std::mem::size_of::<T>())
}

impl Default for PushConstant {
    fn default() -> Self {
        Self {
            pos: [0.; 3],
            time: 0.,
            wh: [1920.0, 780.],
            mouse: [0.; 2],
            mouse_pressed: false as _,
            frame: 0,
            time_delta: 1. / 60.,
            record_period: 10.,
        }
    }
}

impl std::fmt::Display for PushConstant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let time = Duration::from_secs_f32(self.time);
        let time_delta = Duration::from_secs_f32(self.time_delta);
        write!(
            f,
            "position:\t{:?}\n\
             time:\t\t{:#.2?}\n\
             time delta:\t{:#.3?}, fps: {:#.2?}\n\
             width, height:\t{:?}\nmouse:\t\t{:.2?}\n\
             frame:\t\t{}\nrecord_period:\t{}\n",
            self.pos,
            time,
            time_delta,
            1. / self.time_delta,
            self.wh,
            self.mouse,
            self.frame,
            self.record_period
        )
    }
}

#[repr(C)]
#[derive(Default, Debug, Clone, Copy, Pod, Zeroable)]
pub struct Uniform {
    pub pos: [f32; 3],
    _padding: f32,
    pub wh: [f32; 2],
    pub mouse: [f32; 2],
    pub mouse_pressed: u32,
    pub time: f32,
    pub time_delta: f32,
    pub frame: u32,
    pub record_period: f32,
}

impl From<PushConstant> for Uniform {
    fn from(
        PushConstant {
            pos,
            wh,
            mouse,
            mouse_pressed,
            time,
            time_delta,
            frame,
            record_period,
        }: PushConstant,
    ) -> Self {
        Self {
            pos,
            wh,
            mouse,
            mouse_pressed,
            time,
            time_delta,
            frame,
            record_period,
            ..Default::default()
        }
    }
}
