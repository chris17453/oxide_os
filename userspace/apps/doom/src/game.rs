//! Game logic and state management
//!
//! Handles:
//! - Player movement and collision
//! - Game world state
//! - Simple map representation
//!
//! -- GraveShift: Game engine logic - where demons meet their fate

use crate::input::InputState;
use crate::wad::WadFile;
use libc::math::{cosf, sinf};

/// Simple map cell types
const CELL_EMPTY: u8 = 0;
const CELL_WALL: u8 = 1;
const CELL_DOOR: u8 = 2;

/// Game state
pub struct Game {
    player_x: f32,
    player_y: f32,
    player_angle: f32,
    player_health: u32,
    player_ammo: u32,
    map: [[u8; 64]; 64], // Simple 64x64 map
}

impl Game {
    /// Create a new game
    pub fn new(_wad: &WadFile) -> Self {
        let mut game = Game {
            player_x: 5.0,
            player_y: 5.0,
            player_angle: 0.0,
            player_health: 100,
            player_ammo: 50,
            map: [[CELL_EMPTY; 64]; 64],
        };

        // Initialize a simple test map
        game.init_map();
        game
    }

    /// Initialize a simple map for testing
    /// -- GraveShift: Level geometry - procedural hellscape
    fn init_map(&mut self) {
        // Create border walls
        for x in 0..64 {
            self.map[0][x] = CELL_WALL;
            self.map[63][x] = CELL_WALL;
        }
        for y in 0..64 {
            self.map[y][0] = CELL_WALL;
            self.map[y][63] = CELL_WALL;
        }

        // Create some rooms and corridors
        // Room 1 (top-left)
        for x in 10..20 {
            self.map[10][x] = CELL_WALL;
            self.map[20][x] = CELL_WALL;
        }
        for y in 10..20 {
            self.map[y][10] = CELL_WALL;
            self.map[y][20] = CELL_WALL;
        }

        // Room 2 (bottom-right)
        for x in 40..50 {
            self.map[40][x] = CELL_WALL;
            self.map[50][x] = CELL_WALL;
        }
        for y in 40..50 {
            self.map[y][40] = CELL_WALL;
            self.map[y][50] = CELL_WALL;
        }

        // Corridor connecting rooms
        for x in 20..40 {
            self.map[15][x] = CELL_WALL;
            self.map[25][x] = CELL_WALL;
        }
        for y in 15..45 {
            self.map[y][35] = CELL_WALL;
            self.map[y][37] = CELL_WALL;
        }

        // Add some interior walls
        for y in 5..10 {
            self.map[y][30] = CELL_WALL;
        }
        for x in 25..35 {
            self.map[55][x] = CELL_WALL;
        }

        // Doom-esque starter arena using classic E1M1 style bits
        for x in 8..28 {
            self.map[30][x] = CELL_WALL;
        }
        for y in 30..45 {
            self.map[y][28] = CELL_WALL;
        }
        for x in 28..45 {
            self.map[44][x] = CELL_WALL;
        }
        for y in 12..30 {
            self.map[y][44] = CELL_WALL;
        }

        // Doors separating sections
        self.map[30][20] = CELL_DOOR;
        self.map[28][36] = CELL_DOOR;
        self.map[36][44] = CELL_DOOR;
        self.map[44][32] = CELL_DOOR;
    }

    /// Update game state based on input
    /// -- GraveShift: Input processing - react or die
    pub fn update(&mut self, input: &InputState) {
        // Movement speed
        let move_speed = 0.1;
        let turn_speed = 0.05;

        // Turning
        if input.is_left() {
            self.player_angle -= turn_speed;
        }
        if input.is_right() {
            self.player_angle += turn_speed;
        }

        // Movement
        let mut dx = 0.0;
        let mut dy = 0.0;

        if input.is_forward() {
            dx += cosf(self.player_angle) * move_speed;
            dy += sinf(self.player_angle) * move_speed;
        }
        if input.is_backward() {
            dx -= cosf(self.player_angle) * move_speed;
            dy -= sinf(self.player_angle) * move_speed;
        }

        // Strafe
        if input.is_strafe_left() {
            dx += cosf(self.player_angle - 1.5708) * move_speed; // -90 degrees
            dy += sinf(self.player_angle - 1.5708) * move_speed;
        }
        if input.is_strafe_right() {
            dx += cosf(self.player_angle + 1.5708) * move_speed; // +90 degrees
            dy += sinf(self.player_angle + 1.5708) * move_speed;
        }

        // Collision detection
        let new_x = self.player_x + dx;
        let new_y = self.player_y + dy;

        if !self.is_wall(new_x as i32, self.player_y as i32) {
            self.player_x = new_x;
        }
        if !self.is_wall(self.player_x as i32, new_y as i32) {
            self.player_y = new_y;
        }

        // Use/interact
        if input.is_use() {
            // Check for doors in front of player
            let check_x = (self.player_x + cosf(self.player_angle) * 1.5) as i32;
            let check_y = (self.player_y + sinf(self.player_angle) * 1.5) as i32;

            if check_x >= 0 && check_x < 64 && check_y >= 0 && check_y < 64 {
                if self.map[check_y as usize][check_x as usize] == CELL_DOOR {
                    self.map[check_y as usize][check_x as usize] = CELL_EMPTY;
                }
            }
        }

        // Fire weapon
        if input.is_fire() && self.player_ammo > 0 {
            self.player_ammo -= 1;
            // TODO: Add weapon firing logic
        }
    }

    /// Check if a position is a wall
    pub fn is_wall(&self, x: i32, y: i32) -> bool {
        if x < 0 || x >= 64 || y < 0 || y >= 64 {
            return true;
        }
        self.map[y as usize][x as usize] != CELL_EMPTY
    }

    /// Get wall type at position
    pub fn wall_type(&self, x: i32, y: i32) -> u8 {
        if x < 0 || x >= 64 || y < 0 || y >= 64 {
            return CELL_WALL;
        }
        self.map[y as usize][x as usize]
    }

    // Getters for renderer
    pub fn player_x(&self) -> f32 {
        self.player_x
    }
    pub fn player_y(&self) -> f32 {
        self.player_y
    }
    pub fn player_angle(&self) -> f32 {
        self.player_angle
    }
    pub fn player_health(&self) -> u32 {
        self.player_health
    }
    pub fn player_ammo(&self) -> u32 {
        self.player_ammo
    }
}
