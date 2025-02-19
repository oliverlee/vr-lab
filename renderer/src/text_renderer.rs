use crate::*;

pub struct Renderer {
    pub program: rendering::Program,
    pub dimensions_loc: gl::OptionUniformLocation,
    pub text_sampler_loc: gl::OptionUniformLocation,
    pub text_dimensions_loc: gl::OptionUniformLocation,
}

impl Renderer {
    pub fn render(&mut self, gl: &gl::Gl, world: &mut World, context: &FontContext, text_box: &TextBox) {
        unsafe {
            gl.named_buffer_data(context.vb, text_box.vertices.vec_as_bytes(), gl::STREAM_DRAW);
            gl.named_buffer_data(context.eb, text_box.indices.vec_as_bytes(), gl::STREAM_DRAW);

            self.update(gl, world);
            if let ProgramName::Linked(name) = self.program.name {
                gl.disable(gl::DEPTH_TEST);
                gl.depth_mask(gl::FALSE);
                gl.enable(gl::BLEND);
                gl.blend_func(gl::SRC_ALPHA, gl::ONE);

                gl.use_program(name);
                gl.bind_vertex_array(context.vao);

                if let Some(loc) = self.dimensions_loc.into() {
                    gl.uniform_2f(loc, [world.win_size.width as f32, world.win_size.height as f32]);
                }

                if let Some(loc) = self.text_sampler_loc.into() {
                    gl.uniform_1i(loc, 0);
                }

                if let Some(loc) = self.text_dimensions_loc.into() {
                    gl.uniform_2f(loc, [context.meta.scale_x as f32, context.meta.scale_y as f32]);
                }

                // TODO: Handle more than 1 page.
                gl.bind_texture_unit(0, context.pages[0].texture_name);

                gl.draw_elements(gl::TRIANGLES, text_box.indices.len() as u32, gl::UNSIGNED_INT, 0);

                gl.unbind_vertex_array();
                gl.unuse_program();

                gl.enable(gl::DEPTH_TEST);
                gl.depth_mask(gl::TRUE);
                gl.disable(gl::BLEND);
            }
        }
    }

    pub fn update(&mut self, gl: &gl::Gl, world: &mut World) {
        if self.program.update(gl, world) {
            if let ProgramName::Linked(name) = self.program.name {
                unsafe {
                    self.dimensions_loc = get_uniform_location!(gl, name, "dimensions");
                    self.text_dimensions_loc = get_uniform_location!(gl, name, "text_dimensions");
                }
            }
        }
    }

    pub fn new(gl: &gl::Gl, world: &mut World) -> Self {
        Renderer {
            program: vs_fs_program(gl, world, "text_renderer.vert", "text_renderer.frag"),
            dimensions_loc: gl::OptionUniformLocation::NONE,
            text_sampler_loc: gl::OptionUniformLocation::NONE,
            text_dimensions_loc: gl::OptionUniformLocation::NONE,
        }
    }
}
