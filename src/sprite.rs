use std::io::Cursor;

pub const RAW_SPRITE_SHEET: &[u8] = include_bytes!("../assets/cat_sheet.png");
pub const FRAME_WIDTH: usize = 32;
pub const FRAME_HEIGHT: usize = 32;
pub const TOTAL_FRAMES: usize = 16;

/// Facing direction of the cat sprite.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FacingDirection {
    Right,
    Left,
}

/// Pre-processed, integer nearest-neighbor scaled, premultiplied-alpha BGRA sprite bank.
#[derive(Clone)]
pub struct SpriteBank {
    pub scale: u32,
    pub scaled_width: usize,
    pub scaled_height: usize,
    /// 16 frames facing Right, stored as raw 32-bit premultiplied BGRA scanlines.
    frames_right: Vec<Vec<u8>>,
    /// 16 pre-mirrored frames facing Left, stored as raw 32-bit premultiplied BGRA scanlines.
    frames_left: Vec<Vec<u8>>,
}

impl SpriteBank {
    /// Decodes the embedded 512x32 cat sprite sheet and generates nearest-neighbor
    /// scaled, premultiplied-BGRA frames for both left and right facing directions.
    pub fn new(scale: u32) -> Self {
        let scale = scale.max(1);
        let scaled_width = FRAME_WIDTH * scale as usize;
        let scaled_height = FRAME_HEIGHT * scale as usize;

        // 1. Decode PNG into raw RGBA8 pixels
        let decoder = png::Decoder::new(Cursor::new(RAW_SPRITE_SHEET));
        let mut reader = decoder.read_info().expect("Invalid embedded sprite sheet PNG");
        let mut raw_rgba = vec![0u8; reader.output_buffer_size()];
        let output_info = reader.next_frame(&mut raw_rgba).expect("Failed to decode PNG frame");
        raw_rgba.truncate(output_info.buffer_size());

        let sheet_width = output_info.width as usize; // 512
        let _sheet_height = output_info.height as usize; // 32

        let mut frames_right = Vec::with_capacity(TOTAL_FRAMES);
        let mut frames_left = Vec::with_capacity(TOTAL_FRAMES);

        // 2. Extract each 32x32 frame and scale it
        for frame_idx in 0..TOTAL_FRAMES {
            let src_x_offset = frame_idx * FRAME_WIDTH;

            let mut bgra_right = vec![0u8; scaled_width * scaled_height * 4];
            let mut bgra_left = vec![0u8; scaled_width * scaled_height * 4];

            for sy in 0..FRAME_HEIGHT {
                for sx in 0..FRAME_WIDTH {
                    let sheet_x = src_x_offset + sx;
                    let sheet_y = sy;
                    let pixel_idx = (sheet_y * sheet_width + sheet_x) * 4;

                    let r = raw_rgba[pixel_idx] as u32;
                    let g = raw_rgba[pixel_idx + 1] as u32;
                    let b = raw_rgba[pixel_idx + 2] as u32;
                    let a = raw_rgba[pixel_idx + 3] as u32;

                    // Premultiplied alpha calculation:
                    // B' = (B * A) / 255, G' = (G * A) / 255, R' = (R * A) / 255
                    let premult_b = ((b * a) / 255) as u8;
                    let premult_g = ((g * a) / 255) as u8;
                    let premult_r = ((r * a) / 255) as u8;
                    let premult_a = a as u8;

                    // Integer nearest-neighbor replication
                    for dy in 0..scale as usize {
                        for dx in 0..scale as usize {
                            let out_y = sy * scale as usize + dy;
                            let out_x_right = sx * scale as usize + dx;
                            // Horizontally flipped X coordinate for facing Left
                            let out_x_left = (FRAME_WIDTH - 1 - sx) * scale as usize + dx;

                            let right_offset = (out_y * scaled_width + out_x_right) * 4;
                            bgra_right[right_offset] = premult_b;
                            bgra_right[right_offset + 1] = premult_g;
                            bgra_right[right_offset + 2] = premult_r;
                            bgra_right[right_offset + 3] = premult_a;

                            let left_offset = (out_y * scaled_width + out_x_left) * 4;
                            bgra_left[left_offset] = premult_b;
                            bgra_left[left_offset + 1] = premult_g;
                            bgra_left[left_offset + 2] = premult_r;
                            bgra_left[left_offset + 3] = premult_a;
                        }
                    }
                }
            }

            frames_right.push(bgra_right);
            frames_left.push(bgra_left);
        }

        Self {
            scale,
            scaled_width,
            scaled_height,
            frames_right,
            frames_left,
        }
    }

    /// Retrieves the raw 32-bit premultiplied BGRA buffer for a specific frame and direction.
    pub fn get_frame(&self, frame_index: usize, direction: FacingDirection) -> &[u8] {
        let idx = frame_index % TOTAL_FRAMES;
        match direction {
            FacingDirection::Right => &self.frames_right[idx],
            FacingDirection::Left => &self.frames_left[idx],
        }
    }

    pub fn width(&self) -> i32 {
        self.scaled_width as i32
    }

    pub fn height(&self) -> i32 {
        self.scaled_height as i32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sprite_bank_decoding() {
        let bank = SpriteBank::new(2);
        assert_eq!(bank.scaled_width, 64);
        assert_eq!(bank.scaled_height, 64);

        let frame0 = bank.get_frame(0, FacingDirection::Right);
        assert_eq!(frame0.len(), 64 * 64 * 4);

        let frame_mirrored = bank.get_frame(0, FacingDirection::Left);
        assert_eq!(frame_mirrored.len(), 64 * 64 * 4);
    }
}
