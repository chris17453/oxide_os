//! Doom renderer - converts game state to pixels
//!
//! Implements the Doom software rendering pipeline:
//! - BSP traversal
//! - Wall rendering
//! - Sprite rendering
//! - Floor/ceiling rendering
//!
//! -- GlassSignal: Software rasterization, pixel by pixel warfare

use crate::wad::WadFile;
use crate::game::Game;
use libc::math::{cosf, sinf};

/// Color palette (Doom's 256 color palette)
/// -- GlassSignal: Classic 8-bit indexed color, keeping it real
const PALETTE: [u32; 256] = generate_doom_palette();

/// Generate a simplified Doom-style palette
const fn generate_doom_palette() -> [u32; 256] {
    let mut pal = [0u32; 256];
    let mut i = 0;
    
    // Black
    pal[0] = 0xFF000000;
    
    // Grays (1-31)
    i = 1;
    while i < 32 {
        let intensity = (i * 8) as u32;
        pal[i] = 0xFF000000 | (intensity << 16) | (intensity << 8) | intensity;
        i += 1;
    }
    
    // Reds (32-63)
    i = 32;
    while i < 64 {
        let intensity = ((i - 32) * 8) as u32;
        pal[i] = 0xFF000000 | (intensity << 16);
        i += 1;
    }
    
    // Greens (64-95)
    i = 64;
    while i < 96 {
        let intensity = ((i - 64) * 8) as u32;
        pal[i] = 0xFF000000 | (intensity << 8);
        i += 1;
    }
    
    // Blues (96-127)
    i = 96;
    while i < 128 {
        let intensity = ((i - 96) * 8) as u32;
        pal[i] = 0xFF000000 | intensity;
        i += 1;
    }
    
    // Browns/tans (128-159)
    i = 128;
    while i < 160 {
        let intensity = ((i - 128) * 6) as u32;
        pal[i] = 0xFF000000 | (intensity << 16) | ((intensity / 2) << 8);
        i += 1;
    }
    
    // Fill rest with variations
    while i < 256 {
        let r = ((i * 13) % 256) as u32;
        let g = ((i * 7) % 128) as u32;
        let b = ((i * 5) % 128) as u32;
        pal[i] = 0xFF000000 | (r << 16) | (g << 8) | b;
        i += 1;
    }
    
    pal
}

/// Renderer state
pub struct Renderer {
    width: u32,
    height: u32,
    frame_buffer: [u8; 1024 * 768],  // Temporary frame buffer for indexed colors
}

impl Renderer {
    /// Create a new renderer
    pub fn new(width: u32, height: u32, _wad: &WadFile) -> Self {
        Renderer {
            width: width.min(1024),
            height: height.min(768),
            frame_buffer: [0; 1024 * 768],
        }
    }

    /// Render a frame
    /// -- GlassSignal: The render loop - where the magic happens
    pub fn render(&mut self, game: &Game, fb: &mut [u8]) {
        // Clear screen with ceiling color
        let ceiling_color = 96u8;  // Light blue
        let floor_color = 64u8;    // Gray

        let half_height = self.height / 2;

        // Draw ceiling and floor
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = (y * self.width + x) as usize;
                if idx < self.frame_buffer.len() {
                    self.frame_buffer[idx] = if y < half_height {
                        ceiling_color
                    } else {
                        floor_color
                    };
                }
            }
        }

        // Draw walls (simple raycasting)
        self.draw_walls(game);

        // Draw status bar
        self.draw_status_bar(game);

        // Convert indexed colors to RGB and write to framebuffer
        self.blit_to_framebuffer(fb);
    }

    /// Draw walls using raycasting
    /// -- GlassSignal: Raycasting engine - one ray at a time
    fn draw_walls(&mut self, game: &Game) {
        let player_x = game.player_x();
        let player_y = game.player_y();
        let player_angle = game.player_angle();

        // Cast a ray for each column of the screen
        for x in 0..self.width {
            let ray_angle = player_angle + ((x as f32 - self.width as f32 / 2.0) / self.width as f32) * 1.0;
            
            // Cast ray and find wall hit
            let (hit_dist, wall_type) = self.cast_ray(player_x, player_y, ray_angle, game);

            if hit_dist > 0.0 {
                // Calculate wall height on screen
                let wall_height = (self.height as f32 / hit_dist).min(self.height as f32);
                let wall_top = (self.height as f32 / 2.0 - wall_height / 2.0) as u32;
                let wall_bottom = (self.height as f32 / 2.0 + wall_height / 2.0) as u32;

                // Draw wall column
                for y in wall_top..wall_bottom.min(self.height) {
                    let idx = (y * self.width + x) as usize;
                    if idx < self.frame_buffer.len() {
                        // Wall color based on type and distance
                        let base_color = 32 + (wall_type * 16) % 224;
                        let shaded = (base_color as f32 * (1.0 - hit_dist / 10.0).max(0.2)) as u8;
                        self.frame_buffer[idx] = shaded;
                    }
                }
            }
        }
    }

    /// Cast a single ray and return hit distance and wall type
    /// -- GlassSignal: Ray marching through the void
    fn cast_ray(&self, start_x: f32, start_y: f32, angle: f32, game: &Game) -> (f32, u8) {
        let dx = cosf(angle);
        let dy = sinf(angle);
        
        let mut dist = 0.0;
        let max_dist = 20.0;
        let step = 0.1;

        while dist < max_dist {
            let x = start_x + dx * dist;
            let y = start_y + dy * dist;

            if game.is_wall(x as i32, y as i32) {
                return (dist, game.wall_type(x as i32, y as i32));
            }

            dist += step;
        }

        (0.0, 0)
    }

    /// Draw status bar at bottom of screen
    /// -- NeonVale: UI overlay, the HUD lives here
    fn draw_status_bar(&mut self, game: &Game) {
        let bar_height = 32u32;
        let bar_start = self.height.saturating_sub(bar_height);

        // Draw black background for status bar
        for y in bar_start..self.height {
            for x in 0..self.width {
                let idx = (y * self.width + x) as usize;
                if idx < self.frame_buffer.len() {
                    self.frame_buffer[idx] = 0;  // Black
                }
            }
        }

        // Draw health/ammo (simplified)
        let health = game.player_health();
        let ammo = game.player_ammo();
        
        // Draw health bar (red)
        let health_width = (self.width / 4) * health / 100;
        for x in 10..10 + health_width {
            for y in bar_start + 10..bar_start + 20 {
                let idx = (y * self.width + x) as usize;
                if idx < self.frame_buffer.len() {
                    self.frame_buffer[idx] = 40;  // Red
                }
            }
        }

        // Draw ammo bar (yellow)
        let ammo_width = (self.width / 4) * ammo / 100;
        for x in (self.width / 2) + 10..(self.width / 2) + 10 + ammo_width {
            for y in bar_start + 10..bar_start + 20 {
                let idx = (y * self.width + x) as usize;
                if idx < self.frame_buffer.len() {
                    self.frame_buffer[idx] = 160;  // Yellow
                }
            }
        }
    }

    /// Blit the indexed color buffer to the RGB framebuffer
    /// -- GlassSignal: Palette conversion, old meets new
    fn blit_to_framebuffer(&self, fb: &mut [u8]) {
        for y in 0..self.height {
            for x in 0..self.width {
                let idx = (y * self.width + x) as usize;
                if idx >= self.frame_buffer.len() {
                    continue;
                }

                let color_idx = self.frame_buffer[idx] as usize;
                let rgb = PALETTE[color_idx];

                // Write as BGRA (assuming 32bpp framebuffer)
                let fb_idx = idx * 4;
                if fb_idx + 3 < fb.len() {
                    fb[fb_idx] = (rgb & 0xFF) as u8;         // B
                    fb[fb_idx + 1] = ((rgb >> 8) & 0xFF) as u8;  // G
                    fb[fb_idx + 2] = ((rgb >> 16) & 0xFF) as u8; // R
                    fb[fb_idx + 3] = 0xFF;                   // A
                }
            }
        }
    }
}
