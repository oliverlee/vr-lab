#[derive(Debug, Copy, Clone)]
#[repr(C)]
pub struct Entry {
    pub simulation_start_nanos: u64,
    pub simulation_end_pose_start_nanos: u64,
    pub pose_end_render_start_nanos: u64,
    pub render_end_nanos: u64,
}

impl Entry {
    pub fn simulation_duration_nanos(&self) -> u64 {
        self.simulation_end_pose_start_nanos - self.simulation_start_nanos
    }

    pub fn pose_duration_nanos(&self) -> u64 {
        self.pose_end_render_start_nanos - self.simulation_end_pose_start_nanos
    }

    pub fn render_duration_nanos(&self) -> u64 {
        self.render_end_nanos - self.pose_end_render_start_nanos
    }

    pub fn from_ne_bytes(bytes: [u8; std::mem::size_of::<Entry>()]) -> Self {
        unsafe { std::mem::transmute(bytes) }
    }

    pub fn into_ne_bytes(self) -> [u8; std::mem::size_of::<Entry>()] {
        unsafe { std::mem::transmute(self) }
    }
}
