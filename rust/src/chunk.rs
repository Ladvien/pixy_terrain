/// Default vertex color: full weight on texture 0.
pub const DEFAULT_TEXTURE_COLOR: [f32; 4] = [1.0, 0.0, 0.0, 0.0];

/// Integer coordinate identifying a chunk in the world grid
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct ChunkCoord {
    pub x: i32,
    pub y: i32,
    pub z: i32,
}

impl ChunkCoord {
    pub const fn new(x: i32, y: i32, z: i32) -> Self {
        Self { x, y, z }
    }

    /// Convert chunk coordinate to world position (corner of chunk)
    pub fn to_world_position(&self, chunk_size: f32) -> [f32; 3] {
        [
            self.x as f32 * chunk_size,
            self.y as f32 * chunk_size,
            self.z as f32 * chunk_size,
        ]
    }

    /// Get chunk center in world space
    pub fn to_world_center(&self, chunk_size: f32) -> [f32; 3] {
        [
            (self.x as f32 + 0.5) * chunk_size,
            (self.y as f32 + 0.5) * chunk_size,
            (self.z as f32 + 0.5) * chunk_size,
        ]
    }

    /// Distance squared from chunk center to world position
    pub fn distance_squared_to(&self, pos: [f32; 3], chunk_size: f32) -> f32 {
        let center = self.to_world_center(chunk_size);
        let dx = center[0] - pos[0];
        let dy = center[1] - pos[1];
        let dz = center[2] - pos[2];
        dx * dx + dy * dy + dz * dz
    }
}

/// Mesh data produced by worker threads.
/// Uses only Rust primitive types
#[derive(Clone)]
pub struct MeshResult {
    pub coord: ChunkCoord,
    pub lod_level: u8,
    pub vertices: Vec<[f32; 3]>,
    pub normals: Vec<[f32; 3]>,
    pub indices: Vec<i32>,
    /// Vertex colors for texture blending (RGBA = texture weights for 4 textures)
    pub colors: Vec<[f32; 4]>,
}

impl MeshResult {
    pub fn is_empty(&self) -> bool {
        self.vertices.is_empty()
    }

}

/// Lifecycle state of chunk
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ChunkState {
    Unloaded,
    Pending,
    Ready,
    Active,
    MarkedForUnload,
}

/// Chunk metadata stored by ChunkManager
pub struct Chunk {
    pub state: ChunkState,
    pub lod_level: u8,
    pub mesh_instance_id: Option<i64>,
    pub last_access_frame: u64,
}

impl Chunk {
    pub fn new(lod_level: u8) -> Self {
        Self {
            state: ChunkState::Unloaded,
            lod_level,
            mesh_instance_id: None,
            last_access_frame: 0,
        }
    }
}
