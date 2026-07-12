//! Zero-copy reader for the MOC3 binary format.
//!
//! # Alignment
//!
//! This module assumes the provided input bytes are 4-byte aligned
//! in order to allow this to zero-copy into arrays of native types.
//!
//! # Endianness
//!
//! Only little-endian MOC3 files are supported as a big-endian MOC3 file has
//! not been seen in the wild.

use std::mem::size_of;

use bytemuck::{Pod, Zeroable, cast_slice};
use glam::Vec2;
use modular_bitfield::{BitfieldSpecifier, bitfield};
use thiserror::Error;
use yoke::Yokeable;

#[derive(BitfieldSpecifier, Debug, Clone, Copy, PartialEq, Eq)]
#[bits = 2]
pub enum BlendMode {
    Normal = 0,
    Additive = 1 << 0,
    Multiplicative = 1 << 1,
}

#[allow(unused_parens)]
mod inner {
    use super::*;
    #[bitfield(filled = false)]
    #[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
    pub struct ArtMeshFlags {
        pub blend_mode: BlendMode,
        pub double_sided: bool,
        pub inverted: bool,
    }
}
pub use inner::ArtMeshFlags;

#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub enum DrawOrderGroupObjectType {
    ArtMesh = 0,
    Part = 1,
}

#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub enum ParameterType {
    Normal = 0,
    BlendShape = 1,
}

#[derive(Debug, Copy, Clone, PartialOrd, Ord, PartialEq, Eq)]
pub enum Version {
    V3_00 = 1,
    V3_03 = 2,
    V4_00 = 3,
    V4_02 = 4,
}

/// A fixed 64-byte name field, NUL-padded.
#[repr(transparent)]
#[derive(Clone, Copy, Pod, Zeroable, PartialEq, Eq)]
pub struct Id(pub [u8; 64]);

impl Id {
    /// The raw name bytes, up to (not including) the first NUL.
    #[inline]
    pub fn name_bytes(&self) -> &[u8] {
        let end = self.0.iter().position(|&b| b == 0).unwrap_or(self.0.len());
        &self.0[..end]
    }

    /// The name as a `&str`, borrowed from the original data.
    ///
    /// Returns `""` if the name is not valid UTF-8.
    #[inline]
    pub fn name(&self) -> &str {
        core::str::from_utf8(self.name_bytes()).unwrap_or("")
    }
}

impl core::fmt::Debug for Id {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        core::fmt::Debug::fmt(self.name(), f)
    }
}

/// Error returned by [`Moc3::new`].
#[derive(Error, Debug)]
pub enum Moc3Error {
    /// The data was too small to contain a header and section offset table.
    #[error("data is too small to be a moc3 file")]
    TooSmall,
    /// The file did not start with the `MOC3` magic.
    #[error("missing MOC3 magic")]
    BadMagic,
    /// The version byte was not a known version.
    #[error("unknown moc3 version byte: {0}")]
    UnknownVersion(u8),
    /// The file is big-endian, which is not supported.
    #[error("big-endian moc3 files are not supported")]
    BigEndianUnsupported,
    /// The input buffer is not aligned, so sections cannot be reinterpreted as
    /// native slices. The buffer must be at least 4-byte aligned.
    #[error("input buffer is not 4-byte aligned")]
    BufferMisaligned,
    /// A pointer or count implied an offset outside the data (including a
    /// section running past the end of the file), or an arithmetic overflow.
    #[error("an offset pointed outside the file")]
    BadOffset,
    /// A section's data is not aligned to its element size.
    #[error("section data is not correctly aligned")]
    SectionMisaligned,
}

/// The canvas metadata block.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct CanvasInfo {
    pub pixels_per_unit: f32,
    pub x_origin: f32,
    pub y_origin: f32,
    pub canvas_width: f32,
    pub canvas_height: f32,
    pub canvas_flags: u8,
}

/// Zero-copy reader over the count info table: the per-section element counts.
#[derive(Clone, Copy)]
pub struct CountInfo<'a> {
    data: &'a [u8],
    base: usize,
    version: Version,
}

/// Number of count entries in the table: 23 base, plus 9 more from v4.02.
pub(crate) const COUNT_FIELDS_BASE: usize = 23;
pub(crate) const COUNT_FIELDS_V402: usize = 32;

/// Generates a count accessor reading entry `$i` of the table.
///
/// With a leading `$min` version the entry exists only from that version onward,
/// so it reads `0` on older files where the table does not contain it.
macro_rules! count {
    ($(#[$m:meta])* $name:ident, $i:expr) => {
        $(#[$m])*
        #[inline]
        pub fn $name(&self) -> u32 {
            self.at($i)
        }
    };
    ($(#[$m:meta])* $min:path, $name:ident, $i:expr) => {
        $(#[$m])*
        #[inline]
        pub fn $name(&self) -> u32 {
            if self.version >= $min {
                self.at($i)
            } else {
                0
            }
        }
    };
}

impl<'a> CountInfo<'a> {
    /// Build the reader, validating that the whole count table is in bounds.
    fn new(data: &'a [u8], version: Version) -> Result<Self, Moc3Error> {
        let base = read_u32(data, OFF_COUNT_INFO) as usize;
        let fields = if version >= Version::V4_02 {
            COUNT_FIELDS_V402
        } else {
            COUNT_FIELDS_BASE
        };
        let end = base.checked_add(fields * 4).ok_or(Moc3Error::BadOffset)?;
        if end > data.len() {
            return Err(Moc3Error::BadOffset);
        }
        Ok(CountInfo {
            data,
            base,
            version,
        })
    }

    #[inline]
    fn at(&self, index: usize) -> u32 {
        read_u32(self.data, self.base + index * 4)
    }

    count!(parts, 0);
    count!(deformers, 1);
    count!(warp_deformers, 2);
    count!(rotation_deformers, 3);
    count!(art_meshes, 4);
    count!(parameters, 5);
    count!(part_keyforms, 6);
    count!(warp_deformer_keyforms, 7);
    count!(rotation_deformer_keyforms, 8);
    count!(art_mesh_keyforms, 9);
    count!(keyform_positions, 10);
    count!(parameter_binding_indices, 11);
    count!(keyform_bindings, 12);
    count!(parameter_bindings, 13);
    count!(keys, 14);
    count!(uvs, 15);
    count!(vertex_indices, 16);
    count!(art_mesh_masks, 17);
    count!(draw_order_groups, 18);
    count!(draw_order_group_objects, 19);
    count!(glues, 20);
    count!(glue_infos, 21);
    count!(glue_keyforms, 22);

    count!(Version::V4_02, keyform_multiply_colors, 23);
    count!(Version::V4_02, keyform_screen_colors, 24);
    count!(Version::V4_02, blend_shape_parameter_bindings, 25);
    count!(Version::V4_02, blend_shape_keyform_bindings, 26);
    count!(Version::V4_02, blend_shape_warp_deformers, 27);
    count!(Version::V4_02, blend_shape_art_meshes, 28);
    count!(Version::V4_02, blend_shape_constraint_indices, 29);
    count!(Version::V4_02, blend_shape_constraints, 30);
    count!(Version::V4_02, blend_shape_constraint_values, 31);
}

impl core::fmt::Debug for CountInfo<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("CountInfo")
            .field("parts", &self.parts())
            .field("deformers", &self.deformers())
            .field("warp_deformers", &self.warp_deformers())
            .field("rotation_deformers", &self.rotation_deformers())
            .field("art_meshes", &self.art_meshes())
            .field("parameters", &self.parameters())
            .field("keyform_positions", &self.keyform_positions())
            .field("keys", &self.keys())
            .field("uvs", &self.uvs())
            .field("vertex_indices", &self.vertex_indices())
            .field("glues", &self.glues())
            .finish_non_exhaustive()
    }
}

/// Start of the section offset table (immediately after the 64-byte header).
pub(crate) const TABLE: usize = 64;

/// Physical size of the section offset table, in slots. This is sized
/// for v5 files, even though only v4.02 files are currently supported.
pub(crate) const TABLE_SLOTS: usize = 160;

/// The runtime data starts immediately after the full offset table.
pub(crate) const RUNTIME_DATA_START: usize = TABLE + TABLE_SLOTS * 4; // 704

/// Bytes reserved for the header, the offset table, and the runtime data.
pub(crate) const HEADER_RESERVE: usize = 1984;

pub(crate) const MIN_FILE_SIZE: usize = 4096;

pub(crate) const OFF_COUNT_INFO: usize = TABLE; // 64
pub(crate) const OFF_CANVAS_INFO: usize = TABLE + 4; // 68

// first field (`data`) is an inline placeholder
pub(crate) const OFF_PARTS: usize = 72;
pub(crate) const OFF_DEFORMERS: usize = 104;
pub(crate) const OFF_WARP_DEFORMERS: usize = 140;
pub(crate) const OFF_ROTATION_DEFORMERS: usize = 164;
// `runtime_ignored: [u32; 4]` + 16 pointers = 20 slots (80 bytes).
pub(crate) const OFF_ART_MESHES: usize = 180;
pub(crate) const OFF_PARAMETERS: usize = 260;
pub(crate) const OFF_PART_KEYFORMS: usize = 296;
pub(crate) const OFF_WARP_DEFORMER_KEYFORMS: usize = 300;
pub(crate) const OFF_ROTATION_DEFORMER_KEYFORMS: usize = 308;
pub(crate) const OFF_ART_MESH_KEYFORMS: usize = 336;
pub(crate) const OFF_KEYFORM_POSITIONS: usize = 348;
pub(crate) const OFF_PARAMETER_BINDING_INDICES: usize = 352;
pub(crate) const OFF_KEYFORM_BINDINGS: usize = 356;
pub(crate) const OFF_PARAMETER_BINDINGS: usize = 364;
pub(crate) const OFF_KEYS: usize = 372;
pub(crate) const OFF_UVS: usize = 376;
pub(crate) const OFF_VERTEX_INDICES: usize = 380;
pub(crate) const OFF_ART_MESH_MASKS: usize = 384;
pub(crate) const OFF_DRAW_ORDER_GROUPS: usize = 388;
pub(crate) const OFF_DRAW_ORDER_GROUP_OBJECTS: usize = 408;
pub(crate) const OFF_GLUES: usize = 420;
pub(crate) const OFF_GLUE_INFOS: usize = 456;
pub(crate) const OFF_GLUE_KEYFORMS: usize = 464;

// v3.03+
pub(crate) const OFF_WARP_DEFORMER_KEYFORMS_V303: usize = 468;

// v4.02+
pub(crate) const OFF_PARAMETER_EXTENSIONS: usize = 472;
pub(crate) const OFF_WARP_DEFORMER_KEYFORMS_V402: usize = 484;
pub(crate) const OFF_ROTATION_DEFORMER_KEYFORMS_V402: usize = 488;
pub(crate) const OFF_ART_MESH_KEYFORMS_V402: usize = 492;
pub(crate) const OFF_KEYFORM_MULTIPLY_COLORS: usize = 496;
pub(crate) const OFF_KEYFORM_SCREEN_COLORS: usize = 508;
pub(crate) const OFF_PARAMETERS_V402: usize = 520;
pub(crate) const OFF_BLEND_SHAPE_PARAMETER_BINDINGS: usize = 532;
pub(crate) const OFF_BLEND_SHAPE_KEYFORM_BINDINGS: usize = 544;
pub(crate) const OFF_BLEND_SHAPE_WARP_DEFORMERS: usize = 564;
pub(crate) const OFF_BLEND_SHAPE_ART_MESHES: usize = 576;
pub(crate) const OFF_BLEND_SHAPE_CONSTRAINT_INDICES: usize = 588;
pub(crate) const OFF_BLEND_SHAPE_CONSTRAINTS: usize = 592;
pub(crate) const OFF_BLEND_SHAPE_CONSTRAINT_VALUES: usize = 604;

#[inline]
fn read_u32(data: &[u8], off: usize) -> u32 {
    u32::from_le_bytes(data[off..off + 4].try_into().unwrap())
}

#[inline]
fn read_f32(data: &[u8], off: usize) -> f32 {
    f32::from_le_bytes(data[off..off + 4].try_into().unwrap())
}

/// The byte just past the end of the section offset table for `version`. The
/// whole table must be present for any field pointer to be readable.
pub(crate) fn table_end(version: Version) -> usize {
    if version >= Version::V4_02 {
        OFF_BLEND_SHAPE_CONSTRAINT_VALUES + 8
    } else if version >= Version::V3_03 {
        OFF_WARP_DEFORMER_KEYFORMS_V303 + 4
    } else {
        OFF_GLUE_KEYFORMS + 4
    }
}

/// Validate that a single array referenced by the offset table fits within
/// `data` and is aligned for elements of type `T`.
///
/// * `base` – the base address of `data` in the process address space,
///   used only for alignment checks.
/// * `off`   – byte offset within `data` of the 4-byte file pointer.
/// * `count` – expected number of `T` elements.
fn check_section<T>(data: &[u8], base: usize, off: usize, count: usize) -> Result<(), Moc3Error> {
    if count == 0 {
        return Ok(());
    }
    let ptr = read_u32(data, off) as usize;
    let bytes = count
        .checked_mul(std::mem::size_of::<T>())
        .ok_or(Moc3Error::BadOffset)?;
    let end = ptr.checked_add(bytes).ok_or(Moc3Error::BadOffset)?;
    if end > data.len() {
        return Err(Moc3Error::BadOffset);
    }

    // All sections must be at least 8-byte aligned
    if (base + ptr) % usize::max(std::mem::align_of::<T>(), 8) != 0 {
        return Err(Moc3Error::SectionMisaligned);
    }
    Ok(())
}

/// Validate that every array referenced by the offset table fits within `data`
/// and lands at an address aligned to its element type. After this succeeds the
/// accessors can reinterpret the bytes as native slices without copying or
/// risking a panic.
///
/// The set of arrays checked here mirrors the accessors below exactly.
fn validate(data: &[u8], c: CountInfo) -> Result<(), Moc3Error> {
    let base = data.as_ptr() as usize;

    let parts = c.parts() as usize;
    check_section::<Id>(data, base, OFF_PARTS + 4, parts)?;
    for k in [8, 12, 16, 20, 24] {
        check_section::<u32>(data, base, OFF_PARTS + k, parts)?;
    }
    check_section::<i32>(data, base, OFF_PARTS + 28, parts)?; // parent_part_indices

    let deformers = c.deformers() as usize;
    check_section::<Id>(data, base, OFF_DEFORMERS + 4, deformers)?;
    for k in [8, 12, 16] {
        check_section::<u32>(data, base, OFF_DEFORMERS + k, deformers)?;
    }
    check_section::<i32>(data, base, OFF_DEFORMERS + 20, deformers)?; // parent_part_indices
    check_section::<i32>(data, base, OFF_DEFORMERS + 24, deformers)?; // parent_deformer_indices
    for k in [28, 32] {
        check_section::<u32>(data, base, OFF_DEFORMERS + k, deformers)?;
    }

    let warp_deformers = c.warp_deformers() as usize;
    for k in [0, 4, 8, 12, 16, 20] {
        check_section::<u32>(data, base, OFF_WARP_DEFORMERS + k, warp_deformers)?;
    }

    let rotation_deformers = c.rotation_deformers() as usize;
    for k in [0, 4, 8] {
        check_section::<u32>(data, base, OFF_ROTATION_DEFORMERS + k, rotation_deformers)?;
    }
    check_section::<f32>(data, base, OFF_ROTATION_DEFORMERS + 12, rotation_deformers)?; // base_angles

    let art_meshes = c.art_meshes() as usize;
    check_section::<Id>(data, base, OFF_ART_MESHES + 16, art_meshes)?;
    for k in [20, 24, 28, 32, 36] {
        check_section::<u32>(data, base, OFF_ART_MESHES + k, art_meshes)?;
    }
    check_section::<i32>(data, base, OFF_ART_MESHES + 40, art_meshes)?; // parent_part_indices
    check_section::<i32>(data, base, OFF_ART_MESHES + 44, art_meshes)?; // parent_deformer_indices
    check_section::<u32>(data, base, OFF_ART_MESHES + 48, art_meshes)?; // texture_nums
    check_section::<u8>(data, base, OFF_ART_MESHES + 52, art_meshes)?; // flags
    for k in [56, 60, 64, 68, 72, 76] {
        check_section::<u32>(data, base, OFF_ART_MESHES + k, art_meshes)?;
    }

    let parameters = c.parameters() as usize;
    check_section::<Id>(data, base, OFF_PARAMETERS + 4, parameters)?;
    for k in [8, 12, 16] {
        check_section::<f32>(data, base, OFF_PARAMETERS + k, parameters)?; // max/min/default values
    }
    for k in [20, 24, 28, 32] {
        check_section::<u32>(data, base, OFF_PARAMETERS + k, parameters)?;
    }

    check_section::<f32>(data, base, OFF_PART_KEYFORMS, c.part_keyforms() as usize)?; // draw_orders

    let wdk = c.warp_deformer_keyforms() as usize;
    check_section::<f32>(data, base, OFF_WARP_DEFORMER_KEYFORMS, wdk)?; // opacities
    check_section::<u32>(data, base, OFF_WARP_DEFORMER_KEYFORMS + 4, wdk)?; // position_sources_starts

    let rdk = c.rotation_deformer_keyforms() as usize;
    for k in [0, 4, 8, 12, 16] {
        check_section::<f32>(data, base, OFF_ROTATION_DEFORMER_KEYFORMS + k, rdk)?; // opacities/angles/origins/scales
    }
    for k in [20, 24] {
        check_section::<u32>(data, base, OFF_ROTATION_DEFORMER_KEYFORMS + k, rdk)?; // is_reflect_x/y
    }

    let amk = c.art_mesh_keyforms() as usize;
    for k in [0, 4] {
        check_section::<f32>(data, base, OFF_ART_MESH_KEYFORMS + k, amk)?; // opacities, draw_orders
    }
    check_section::<u32>(data, base, OFF_ART_MESH_KEYFORMS + 8, amk)?; // position_sources_starts

    check_section::<Vec2>(
        data,
        base,
        OFF_KEYFORM_POSITIONS,
        c.keyform_positions() as usize / 2,
    )?;
    check_section::<u32>(
        data,
        base,
        OFF_PARAMETER_BINDING_INDICES,
        c.parameter_binding_indices() as usize,
    )?;

    let kb = c.keyform_bindings() as usize;
    check_section::<u32>(data, base, OFF_KEYFORM_BINDINGS, kb)?;
    check_section::<u32>(data, base, OFF_KEYFORM_BINDINGS + 4, kb)?;

    let pb = c.parameter_bindings() as usize;
    check_section::<u32>(data, base, OFF_PARAMETER_BINDINGS, pb)?;
    check_section::<u32>(data, base, OFF_PARAMETER_BINDINGS + 4, pb)?;

    check_section::<f32>(data, base, OFF_KEYS, c.keys() as usize)?;
    check_section::<Vec2>(data, base, OFF_UVS, c.uvs() as usize / 2)?;
    check_section::<u16>(data, base, OFF_VERTEX_INDICES, c.vertex_indices() as usize)?;
    check_section::<u32>(data, base, OFF_ART_MESH_MASKS, c.art_mesh_masks() as usize)?;

    let dog = c.draw_order_groups() as usize;
    for k in [0, 4, 8, 12, 16] {
        check_section::<u32>(data, base, OFF_DRAW_ORDER_GROUPS + k, dog)?;
    }

    let dogo = c.draw_order_group_objects() as usize;
    for k in [0, 4] {
        check_section::<u32>(data, base, OFF_DRAW_ORDER_GROUP_OBJECTS + k, dogo)?;
    }
    check_section::<i32>(data, base, OFF_DRAW_ORDER_GROUP_OBJECTS + 8, dogo)?; // self_indices

    let glues = c.glues() as usize;
    check_section::<Id>(data, base, OFF_GLUES + 4, glues)?;
    for k in [8, 12, 16, 20, 24, 28, 32] {
        check_section::<u32>(data, base, OFF_GLUES + k, glues)?;
    }

    let glue_infos = c.glue_infos() as usize;
    check_section::<f32>(data, base, OFF_GLUE_INFOS, glue_infos)?; // weights
    check_section::<u16>(data, base, OFF_GLUE_INFOS + 4, glue_infos)?; // vertex_indices

    check_section::<f32>(data, base, OFF_GLUE_KEYFORMS, c.glue_keyforms() as usize)?; // intensities

    if c.version >= Version::V3_03 {
        check_section::<u32>(data, base, OFF_WARP_DEFORMER_KEYFORMS_V303, warp_deformers)?;
    }

    if c.version >= Version::V4_02 {
        check_section::<u32>(data, base, OFF_PARAMETER_EXTENSIONS + 4, parameters)?;
        check_section::<u32>(data, base, OFF_PARAMETER_EXTENSIONS + 8, parameters)?;
        check_section::<u32>(data, base, OFF_WARP_DEFORMER_KEYFORMS_V402, warp_deformers)?;
        check_section::<u32>(
            data,
            base,
            OFF_ROTATION_DEFORMER_KEYFORMS_V402,
            rotation_deformers,
        )?;
        check_section::<u32>(data, base, OFF_ART_MESH_KEYFORMS_V402, art_meshes)?;

        let mc = c.keyform_multiply_colors() as usize;
        let sc = c.keyform_screen_colors() as usize;
        for k in [0, 4, 8] {
            check_section::<f32>(data, base, OFF_KEYFORM_MULTIPLY_COLORS + k, mc)?; // r/g/b
            check_section::<f32>(data, base, OFF_KEYFORM_SCREEN_COLORS + k, sc)?; // r/g/b
        }

        for k in [0, 4, 8] {
            check_section::<u32>(data, base, OFF_PARAMETERS_V402 + k, parameters)?;
        }

        let bspb = c.blend_shape_parameter_bindings() as usize;
        for k in [0, 4, 8] {
            check_section::<u32>(data, base, OFF_BLEND_SHAPE_PARAMETER_BINDINGS + k, bspb)?;
        }

        let bskb = c.blend_shape_keyform_bindings() as usize;
        for k in [0, 4, 8, 12, 16] {
            check_section::<u32>(data, base, OFF_BLEND_SHAPE_KEYFORM_BINDINGS + k, bskb)?;
        }

        let bswd = c.blend_shape_warp_deformers() as usize;
        let bsam = c.blend_shape_art_meshes() as usize;
        for k in [0, 4, 8] {
            check_section::<u32>(data, base, OFF_BLEND_SHAPE_WARP_DEFORMERS + k, bswd)?;
            check_section::<u32>(data, base, OFF_BLEND_SHAPE_ART_MESHES + k, bsam)?;
        }

        check_section::<u32>(
            data,
            base,
            OFF_BLEND_SHAPE_CONSTRAINT_INDICES,
            c.blend_shape_constraint_indices() as usize,
        )?;

        let bsc = c.blend_shape_constraints() as usize;
        for k in [0, 4, 8] {
            check_section::<u32>(data, base, OFF_BLEND_SHAPE_CONSTRAINTS + k, bsc)?;
        }

        let bscv = c.blend_shape_constraint_values() as usize;
        check_section::<f32>(data, base, OFF_BLEND_SHAPE_CONSTRAINT_VALUES, bscv)?; // keys
        check_section::<f32>(data, base, OFF_BLEND_SHAPE_CONSTRAINT_VALUES + 4, bscv)?; // weights
    }

    Ok(())
}

/// A zero-copy view over the contents of a MOC3 file.
///
/// The `Yokeable` impl (used by [`crate::owned::OwnedMoc3`]) is sound because
/// `Moc3` only holds shared references and is covariant in its lifetime.
#[derive(Clone, Copy, Yokeable)]
pub struct Moc3<'a> {
    data: &'a [u8],
    version: Version,
    counts: CountInfo<'a>,
}

/// Generates an array section accessor.
///
/// `$cf` is the [`CountInfo`] count accessor giving the element count; `$off` is
/// the absolute byte offset of the file pointer; `$t` is the native element type.
///
/// With a leading `$min` version the section exists only from that version
/// onward, so the accessor returns `Option` (`None` on older files). Without it
/// the section is always present and the slice is returned directly.
macro_rules! acc {
    ($(#[$m:meta])* $name:ident, $off:expr, $cf:ident, $t:ty) => {
        $(#[$m])*
        #[inline]
        pub fn $name(&self) -> &'a [$t] {
            self.arr::<$t>($off, self.counts.$cf() as usize)
        }
    };
    ($(#[$m:meta])* $min:path, $name:ident, $off:expr, $cf:ident, $t:ty) => {
        $(#[$m])*
        #[inline]
        pub fn $name(&self) -> Option<&'a [$t]> {
            if self.version >= $min {
                Some(self.arr::<$t>($off, self.counts.$cf() as usize))
            } else {
                None
            }
        }
    };
}

impl<'a> Moc3<'a> {
    /// Parse and validate the header, count table, and every section, returning
    /// a zero-copy view.
    ///
    /// On success, every accessor is guaranteed to be in bounds and correctly
    /// aligned, so the accessors themselves are infallible and never copy.
    ///
    /// # Errors
    ///
    /// Returns an error if the magic / version / endianness flag are invalid,
    /// if the buffer is not 4-byte aligned (see [the module docs](self)), or if
    /// any section's data would run past the end of the file or land at a
    /// misaligned address.
    pub fn new(data: &'a [u8]) -> Result<Self, Moc3Error> {
        // Header is padded to 64 bytes, and the count info pointer lives right
        // after it, so we need at least that much.
        if data.len() < TABLE + 8 {
            return Err(Moc3Error::TooSmall);
        }
        if &data[0..4] != b"MOC3" {
            return Err(Moc3Error::BadMagic);
        }
        let version = match data[4] {
            1 => Version::V3_00,
            2 => Version::V3_03,
            3 => Version::V4_00,
            4 => Version::V4_02,
            v => return Err(Moc3Error::UnknownVersion(v)),
        };
        if data[5] != 0 {
            return Err(Moc3Error::BigEndianUnsupported);
        }

        // Every element type used here has alignment <= 4, so a 4-byte aligned
        // buffer plus element-aligned section offsets guarantees aligned slices.
        if (data.as_ptr() as usize) % 4 != 0 {
            return Err(Moc3Error::BufferMisaligned);
        }

        // The whole offset table must be present so reading any field pointer
        // (in the accessors and in `validate`) cannot go out of bounds.
        if data.len() < table_end(version) {
            return Err(Moc3Error::TooSmall);
        }

        let counts = CountInfo::new(data, version)?;

        validate(data, counts)?;

        Ok(Moc3 {
            data,
            version,
            counts,
        })
    }

    /// The file's format version.
    #[inline]
    pub fn version(&self) -> Version {
        self.version
    }

    /// A zero-copy reader for the per-section element counts.
    #[inline]
    pub fn counts(&self) -> CountInfo<'a> {
        self.counts
    }

    /// The underlying byte slice.
    #[inline]
    pub fn data(&self) -> &'a [u8] {
        self.data
    }

    /// Decode the canvas info block.
    pub fn canvas_info(&self) -> CanvasInfo {
        let p = read_u32(self.data, OFF_CANVAS_INFO) as usize;
        CanvasInfo {
            pixels_per_unit: read_f32(self.data, p),
            x_origin: read_f32(self.data, p + 4),
            y_origin: read_f32(self.data, p + 8),
            canvas_width: read_f32(self.data, p + 12),
            canvas_height: read_f32(self.data, p + 16),
            canvas_flags: self.data[p + 20],
        }
    }

    /// Read the file pointer at `field_off`, then return `count` elements of
    /// type `T` from where it points, as a zero-copy slice.
    ///
    /// This is infallible because [`Moc3::new`] has already validated that every
    /// section is in bounds and correctly aligned; the `cast_slice` below cannot
    /// panic for a view that passed validation.
    #[inline]
    fn arr<T: Pod>(&self, field_off: usize, count: usize) -> &'a [T] {
        if count == 0 {
            return &[];
        }
        let ptr = read_u32(self.data, field_off) as usize;
        let bytes = &self.data[ptr..ptr + count * size_of::<T>()];
        cast_slice(bytes)
    }

    // ----- parts -----
    acc!(part_ids, OFF_PARTS + 4, parts, Id);
    acc!(
        part_keyform_binding_sources_indices,
        OFF_PARTS + 8,
        parts,
        u32
    );
    acc!(part_keyform_sources_starts, OFF_PARTS + 12, parts, u32);
    acc!(part_keyform_sources_counts, OFF_PARTS + 16, parts, u32);
    acc!(part_is_visible, OFF_PARTS + 20, parts, u32);
    acc!(part_is_enabled, OFF_PARTS + 24, parts, u32);
    acc!(part_parent_part_indices, OFF_PARTS + 28, parts, i32);

    // ----- deformers -----
    acc!(deformer_ids, OFF_DEFORMERS + 4, deformers, Id);
    acc!(
        deformer_keyform_binding_sources_indices,
        OFF_DEFORMERS + 8,
        deformers,
        u32
    );
    acc!(deformer_is_visible, OFF_DEFORMERS + 12, deformers, u32);
    acc!(deformer_is_enabled, OFF_DEFORMERS + 16, deformers, u32);
    acc!(
        deformer_parent_part_indices,
        OFF_DEFORMERS + 20,
        deformers,
        i32
    );
    acc!(
        deformer_parent_deformer_indices,
        OFF_DEFORMERS + 24,
        deformers,
        i32
    );
    acc!(
        /// Deformer kind: `0` = warp deformer, `1` = rotation deformer.
        deformer_types, OFF_DEFORMERS + 28, deformers, u32);
    acc!(
        deformer_specific_sources_indices,
        OFF_DEFORMERS + 32,
        deformers,
        u32
    );

    // ----- warp deformers -----
    acc!(
        warp_deformer_keyform_binding_sources_indices,
        OFF_WARP_DEFORMERS,
        warp_deformers,
        u32
    );
    acc!(
        warp_deformer_keyform_sources_starts,
        OFF_WARP_DEFORMERS + 4,
        warp_deformers,
        u32
    );
    acc!(
        warp_deformer_keyform_sources_counts,
        OFF_WARP_DEFORMERS + 8,
        warp_deformers,
        u32
    );
    acc!(
        warp_deformer_vertex_counts,
        OFF_WARP_DEFORMERS + 12,
        warp_deformers,
        u32
    );
    acc!(
        warp_deformer_rows,
        OFF_WARP_DEFORMERS + 16,
        warp_deformers,
        u32
    );
    acc!(
        warp_deformer_columns,
        OFF_WARP_DEFORMERS + 20,
        warp_deformers,
        u32
    );

    // ----- rotation deformers -----
    acc!(
        rotation_deformer_keyform_binding_sources_indices,
        OFF_ROTATION_DEFORMERS,
        rotation_deformers,
        u32
    );
    acc!(
        rotation_deformer_keyform_sources_starts,
        OFF_ROTATION_DEFORMERS + 4,
        rotation_deformers,
        u32
    );
    acc!(
        rotation_deformer_keyform_sources_counts,
        OFF_ROTATION_DEFORMERS + 8,
        rotation_deformers,
        u32
    );
    acc!(
        rotation_deformer_base_angles,
        OFF_ROTATION_DEFORMERS + 12,
        rotation_deformers,
        f32
    );

    // ----- art meshes -----
    acc!(art_mesh_ids, OFF_ART_MESHES + 16, art_meshes, Id);
    acc!(
        art_mesh_keyform_binding_sources_indices,
        OFF_ART_MESHES + 20,
        art_meshes,
        u32
    );
    acc!(
        art_mesh_keyform_sources_starts,
        OFF_ART_MESHES + 24,
        art_meshes,
        u32
    );
    acc!(
        art_mesh_keyform_sources_counts,
        OFF_ART_MESHES + 28,
        art_meshes,
        u32
    );
    acc!(art_mesh_is_visible, OFF_ART_MESHES + 32, art_meshes, u32);
    acc!(art_mesh_is_enabled, OFF_ART_MESHES + 36, art_meshes, u32);
    acc!(
        art_mesh_parent_part_indices,
        OFF_ART_MESHES + 40,
        art_meshes,
        i32
    );
    acc!(
        art_mesh_parent_deformer_indices,
        OFF_ART_MESHES + 44,
        art_meshes,
        i32
    );
    acc!(art_mesh_texture_nums, OFF_ART_MESHES + 48, art_meshes, u32);
    acc!(art_mesh_flags, OFF_ART_MESHES + 52, art_meshes, u8);
    acc!(art_mesh_vertex_counts, OFF_ART_MESHES + 56, art_meshes, u32);
    acc!(
        art_mesh_uv_sources_starts,
        OFF_ART_MESHES + 60,
        art_meshes,
        u32
    );
    acc!(
        art_mesh_vertex_index_sources_starts,
        OFF_ART_MESHES + 64,
        art_meshes,
        u32
    );
    acc!(
        art_mesh_vertex_index_sources_counts,
        OFF_ART_MESHES + 68,
        art_meshes,
        u32
    );
    acc!(
        art_mesh_mask_sources_starts,
        OFF_ART_MESHES + 72,
        art_meshes,
        u32
    );
    acc!(
        art_mesh_mask_sources_counts,
        OFF_ART_MESHES + 76,
        art_meshes,
        u32
    );

    // ----- parameters -----
    acc!(parameter_ids, OFF_PARAMETERS + 4, parameters, Id);
    acc!(parameter_max_values, OFF_PARAMETERS + 8, parameters, f32);
    acc!(parameter_min_values, OFF_PARAMETERS + 12, parameters, f32);
    acc!(
        parameter_default_values,
        OFF_PARAMETERS + 16,
        parameters,
        f32
    );
    acc!(parameter_is_repeat, OFF_PARAMETERS + 20, parameters, u32);
    acc!(
        parameter_decimal_places,
        OFF_PARAMETERS + 24,
        parameters,
        u32
    );
    acc!(
        parameter_binding_sources_starts,
        OFF_PARAMETERS + 28,
        parameters,
        u32
    );
    acc!(
        parameter_binding_sources_counts,
        OFF_PARAMETERS + 32,
        parameters,
        u32
    );

    // ----- keyforms -----
    acc!(
        part_keyform_draw_orders,
        OFF_PART_KEYFORMS,
        part_keyforms,
        f32
    );

    acc!(
        warp_deformer_keyform_opacities,
        OFF_WARP_DEFORMER_KEYFORMS,
        warp_deformer_keyforms,
        f32
    );
    acc!(
        warp_deformer_keyform_position_sources_starts,
        OFF_WARP_DEFORMER_KEYFORMS + 4,
        warp_deformer_keyforms,
        u32
    );

    acc!(
        rotation_deformer_keyform_opacities,
        OFF_ROTATION_DEFORMER_KEYFORMS,
        rotation_deformer_keyforms,
        f32
    );
    acc!(
        rotation_deformer_keyform_angles,
        OFF_ROTATION_DEFORMER_KEYFORMS + 4,
        rotation_deformer_keyforms,
        f32
    );
    acc!(
        rotation_deformer_keyform_x_origin,
        OFF_ROTATION_DEFORMER_KEYFORMS + 8,
        rotation_deformer_keyforms,
        f32
    );
    acc!(
        rotation_deformer_keyform_y_origin,
        OFF_ROTATION_DEFORMER_KEYFORMS + 12,
        rotation_deformer_keyforms,
        f32
    );
    acc!(
        rotation_deformer_keyform_scales,
        OFF_ROTATION_DEFORMER_KEYFORMS + 16,
        rotation_deformer_keyforms,
        f32
    );
    acc!(
        rotation_deformer_keyform_is_reflect_x,
        OFF_ROTATION_DEFORMER_KEYFORMS + 20,
        rotation_deformer_keyforms,
        u32
    );
    acc!(
        rotation_deformer_keyform_is_reflect_y,
        OFF_ROTATION_DEFORMER_KEYFORMS + 24,
        rotation_deformer_keyforms,
        u32
    );

    acc!(
        art_mesh_keyform_opacities,
        OFF_ART_MESH_KEYFORMS,
        art_mesh_keyforms,
        f32
    );
    acc!(
        art_mesh_keyform_draw_orders,
        OFF_ART_MESH_KEYFORMS + 4,
        art_mesh_keyforms,
        f32
    );
    acc!(
        art_mesh_keyform_position_sources_starts,
        OFF_ART_MESH_KEYFORMS + 8,
        art_mesh_keyforms,
        u32
    );

    /// Keyform position coordinates.
    ///
    /// L2D indexes these in units of `f32`, not `Vec2`, so the
    /// `keyform_position_sources_starts` indices must be divided by 2 before
    /// indexing into this slice.
    pub fn positions(&self) -> &'a [Vec2] {
        self.arr::<Vec2>(
            OFF_KEYFORM_POSITIONS,
            self.counts.keyform_positions() as usize / 2,
        )
    }

    acc!(
        parameter_binding_indices,
        OFF_PARAMETER_BINDING_INDICES,
        parameter_binding_indices,
        u32
    );

    acc!(
        keyform_binding_parameter_binding_index_sources_starts,
        OFF_KEYFORM_BINDINGS,
        keyform_bindings,
        u32
    );
    acc!(
        keyform_binding_parameter_binding_index_sources_counts,
        OFF_KEYFORM_BINDINGS + 4,
        keyform_bindings,
        u32
    );

    acc!(
        parameter_binding_keys_sources_starts,
        OFF_PARAMETER_BINDINGS,
        parameter_bindings,
        u32
    );
    acc!(
        parameter_binding_keys_sources_counts,
        OFF_PARAMETER_BINDINGS + 4,
        parameter_bindings,
        u32
    );

    acc!(keys, OFF_KEYS, keys, f32);

    /// UV coordinates. Indexed in units of `f32` like [`positions`](Self::positions).
    pub fn uvs(&self) -> &'a [Vec2] {
        self.arr::<Vec2>(OFF_UVS, self.counts.uvs() as usize / 2)
    }

    acc!(vertex_indices, OFF_VERTEX_INDICES, vertex_indices, u16);

    acc!(
        art_mesh_mask_source_indices,
        OFF_ART_MESH_MASKS,
        art_mesh_masks,
        u32
    );

    // ----- draw order groups -----
    acc!(
        draw_order_group_object_sources_starts,
        OFF_DRAW_ORDER_GROUPS,
        draw_order_groups,
        u32
    );
    acc!(
        draw_order_group_object_sources_counts,
        OFF_DRAW_ORDER_GROUPS + 4,
        draw_order_groups,
        u32
    );
    acc!(
        draw_order_group_object_sources_total_counts,
        OFF_DRAW_ORDER_GROUPS + 8,
        draw_order_groups,
        u32
    );
    acc!(
        draw_order_group_maximum_draw_orders,
        OFF_DRAW_ORDER_GROUPS + 12,
        draw_order_groups,
        u32
    );
    acc!(
        draw_order_group_minimum_draw_orders,
        OFF_DRAW_ORDER_GROUPS + 16,
        draw_order_groups,
        u32
    );

    acc!(
        draw_order_group_object_types,
        OFF_DRAW_ORDER_GROUP_OBJECTS,
        draw_order_group_objects,
        u32
    );
    acc!(
        draw_order_group_object_indices,
        OFF_DRAW_ORDER_GROUP_OBJECTS + 4,
        draw_order_group_objects,
        u32
    );
    acc!(
        draw_order_group_object_self_indices,
        OFF_DRAW_ORDER_GROUP_OBJECTS + 8,
        draw_order_group_objects,
        i32
    );

    // ----- glues -----
    acc!(glue_ids, OFF_GLUES + 4, glues, Id);
    acc!(
        glue_keyform_binding_sources_indices,
        OFF_GLUES + 8,
        glues,
        u32
    );
    acc!(glue_keyform_sources_starts, OFF_GLUES + 12, glues, u32);
    acc!(glue_keyform_sources_counts, OFF_GLUES + 16, glues, u32);
    acc!(glue_art_mesh_indices_a, OFF_GLUES + 20, glues, u32);
    acc!(glue_art_mesh_indices_b, OFF_GLUES + 24, glues, u32);
    acc!(glue_info_sources_starts, OFF_GLUES + 28, glues, u32);
    acc!(glue_info_sources_counts, OFF_GLUES + 32, glues, u32);

    acc!(glue_info_weights, OFF_GLUE_INFOS, glue_infos, f32);
    acc!(
        glue_info_vertex_indices,
        OFF_GLUE_INFOS + 4,
        glue_infos,
        u16
    );

    acc!(
        glue_keyform_intensities,
        OFF_GLUE_KEYFORMS,
        glue_keyforms,
        f32
    );

    // ----- v3.03+ -----
    acc!(
        /// Per warp deformer "is new deformer" flags (v3.03+).
        Version::V3_03,
        warp_deformer_is_new_deformer,
        OFF_WARP_DEFORMER_KEYFORMS_V303,
        warp_deformers,
        u32
    );

    // ----- v4.02+ -----
    acc!(
        Version::V4_02,
        parameter_extension_keys_sources_starts,
        OFF_PARAMETER_EXTENSIONS + 4,
        parameters,
        u32
    );
    acc!(
        Version::V4_02,
        parameter_extension_keys_sources_counts,
        OFF_PARAMETER_EXTENSIONS + 8,
        parameters,
        u32
    );

    acc!(
        Version::V4_02,
        warp_deformer_keyform_color_sources_start,
        OFF_WARP_DEFORMER_KEYFORMS_V402,
        warp_deformers,
        u32
    );
    acc!(
        Version::V4_02,
        rotation_deformer_keyform_color_sources_start,
        OFF_ROTATION_DEFORMER_KEYFORMS_V402,
        rotation_deformers,
        u32
    );
    acc!(
        Version::V4_02,
        art_mesh_keyform_color_sources_start,
        OFF_ART_MESH_KEYFORMS_V402,
        art_meshes,
        u32
    );

    acc!(
        Version::V4_02,
        keyform_multiply_colors_red,
        OFF_KEYFORM_MULTIPLY_COLORS,
        keyform_multiply_colors,
        f32
    );
    acc!(
        Version::V4_02,
        keyform_multiply_colors_green,
        OFF_KEYFORM_MULTIPLY_COLORS + 4,
        keyform_multiply_colors,
        f32
    );
    acc!(
        Version::V4_02,
        keyform_multiply_colors_blue,
        OFF_KEYFORM_MULTIPLY_COLORS + 8,
        keyform_multiply_colors,
        f32
    );

    acc!(
        Version::V4_02,
        keyform_screen_colors_red,
        OFF_KEYFORM_SCREEN_COLORS,
        keyform_screen_colors,
        f32
    );
    acc!(
        Version::V4_02,
        keyform_screen_colors_green,
        OFF_KEYFORM_SCREEN_COLORS + 4,
        keyform_screen_colors,
        f32
    );
    acc!(
        Version::V4_02,
        keyform_screen_colors_blue,
        OFF_KEYFORM_SCREEN_COLORS + 8,
        keyform_screen_colors,
        f32
    );

    acc!(
        Version::V4_02,
        parameter_types,
        OFF_PARAMETERS_V402,
        parameters,
        u32
    );
    acc!(
        Version::V4_02,
        parameter_blend_shape_binding_sources_starts,
        OFF_PARAMETERS_V402 + 4,
        parameters,
        u32
    );
    acc!(
        Version::V4_02,
        parameter_blend_shape_binding_sources_counts,
        OFF_PARAMETERS_V402 + 8,
        parameters,
        u32
    );

    acc!(
        Version::V4_02,
        blend_shape_parameter_binding_keys_sources_starts,
        OFF_BLEND_SHAPE_PARAMETER_BINDINGS,
        blend_shape_parameter_bindings,
        u32
    );
    acc!(
        Version::V4_02,
        blend_shape_parameter_binding_keys_sources_counts,
        OFF_BLEND_SHAPE_PARAMETER_BINDINGS + 4,
        blend_shape_parameter_bindings,
        u32
    );
    acc!(
        Version::V4_02,
        blend_shape_parameter_binding_base_key_indices,
        OFF_BLEND_SHAPE_PARAMETER_BINDINGS + 8,
        blend_shape_parameter_bindings,
        u32
    );

    acc!(
        Version::V4_02,
        blend_shape_keyform_binding_parameter_binding_sources_indices,
        OFF_BLEND_SHAPE_KEYFORM_BINDINGS,
        blend_shape_keyform_bindings,
        u32
    );
    acc!(
        Version::V4_02,
        blend_shape_keyform_binding_keyform_sources_starts,
        OFF_BLEND_SHAPE_KEYFORM_BINDINGS + 4,
        blend_shape_keyform_bindings,
        u32
    );
    acc!(
        Version::V4_02,
        blend_shape_keyform_binding_keyform_sources_counts,
        OFF_BLEND_SHAPE_KEYFORM_BINDINGS + 8,
        blend_shape_keyform_bindings,
        u32
    );
    acc!(
        Version::V4_02,
        blend_shape_keyform_binding_constraint_index_sources_starts,
        OFF_BLEND_SHAPE_KEYFORM_BINDINGS + 12,
        blend_shape_keyform_bindings,
        u32
    );
    acc!(
        Version::V4_02,
        blend_shape_keyform_binding_constraint_index_sources_counts,
        OFF_BLEND_SHAPE_KEYFORM_BINDINGS + 16,
        blend_shape_keyform_bindings,
        u32
    );

    acc!(
        Version::V4_02,
        blend_shape_warp_deformer_target_indices,
        OFF_BLEND_SHAPE_WARP_DEFORMERS,
        blend_shape_warp_deformers,
        u32
    );
    acc!(
        Version::V4_02,
        blend_shape_warp_deformer_keyform_binding_sources_starts,
        OFF_BLEND_SHAPE_WARP_DEFORMERS + 4,
        blend_shape_warp_deformers,
        u32
    );
    acc!(
        Version::V4_02,
        blend_shape_warp_deformer_keyform_binding_sources_counts,
        OFF_BLEND_SHAPE_WARP_DEFORMERS + 8,
        blend_shape_warp_deformers,
        u32
    );

    acc!(
        Version::V4_02,
        blend_shape_art_mesh_target_indices,
        OFF_BLEND_SHAPE_ART_MESHES,
        blend_shape_art_meshes,
        u32
    );
    acc!(
        Version::V4_02,
        blend_shape_art_mesh_keyform_binding_sources_starts,
        OFF_BLEND_SHAPE_ART_MESHES + 4,
        blend_shape_art_meshes,
        u32
    );
    acc!(
        Version::V4_02,
        blend_shape_art_mesh_keyform_binding_sources_counts,
        OFF_BLEND_SHAPE_ART_MESHES + 8,
        blend_shape_art_meshes,
        u32
    );

    acc!(
        Version::V4_02,
        blend_shape_constraint_sources_indices,
        OFF_BLEND_SHAPE_CONSTRAINT_INDICES,
        blend_shape_constraint_indices,
        u32
    );

    acc!(
        Version::V4_02,
        blend_shape_constraint_parameter_indices,
        OFF_BLEND_SHAPE_CONSTRAINTS,
        blend_shape_constraints,
        u32
    );
    acc!(
        Version::V4_02,
        blend_shape_constraint_value_sources_starts,
        OFF_BLEND_SHAPE_CONSTRAINTS + 4,
        blend_shape_constraints,
        u32
    );
    acc!(
        Version::V4_02,
        blend_shape_constraint_value_sources_counts,
        OFF_BLEND_SHAPE_CONSTRAINTS + 8,
        blend_shape_constraints,
        u32
    );

    acc!(
        Version::V4_02,
        blend_shape_constraint_value_keys,
        OFF_BLEND_SHAPE_CONSTRAINT_VALUES,
        blend_shape_constraint_values,
        f32
    );
    acc!(
        Version::V4_02,
        blend_shape_constraint_value_weights,
        OFF_BLEND_SHAPE_CONSTRAINT_VALUES + 4,
        blend_shape_constraint_values,
        f32
    );
}

impl std::fmt::Debug for Moc3<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Moc3")
            .field("version", &self.version)
            .field("len", &self.data.len())
            .field("counts", &self.counts)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_input() {
        assert!(matches!(Moc3::new(&[]), Err(Moc3Error::TooSmall)));
        let mut bad = vec![0u8; 128];
        bad[0..4].copy_from_slice(b"XXXX");
        assert!(matches!(Moc3::new(&bad), Err(Moc3Error::BadMagic)));
        bad[0..4].copy_from_slice(b"MOC3");
        bad[4] = 99;
        assert!(matches!(
            Moc3::new(&bad),
            Err(Moc3Error::UnknownVersion(99))
        ));
        bad[4] = 4;
        bad[5] = 1;
        assert!(matches!(
            Moc3::new(&bad),
            Err(Moc3Error::BigEndianUnsupported)
        ));
    }
}
