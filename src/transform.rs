use crate::Png;
use crate::raw::Palette;
use crate::raw::BitDepth;
use std::borrow::Cow;


#[derive(Debug)]
pub struct TransformError;

pub trait InputTransform {
    fn transform(&self, png: &mut Png) -> Result<(), TransformError>;
}

/// Transform 1, 2 and 4 bit samples to 8 bit samples
pub struct Unpack;

impl Unpack{
    fn unpack(input: &[u8], output: &mut [u8], bitdepth: usize, samples: usize) {
        let mask = ((1u16 << bitdepth) - 1) as u8;

        for i in 0..output.len() {
            let j = i * bitdepth / 8 ; // Input position
            let shift = (samples - (i % samples) - 1) * bitdepth;

            output[i] = ((mask << shift) & input[j]) >> shift;
        }
    }
}

impl InputTransform for Unpack {
    fn transform(&self, png: &mut Png) -> Result<(), TransformError> {
        match png.header.colour.bit_depth {
            BitDepth::B8 | BitDepth::B16 => {},
            b => {
                let samples = 8 / b as usize; // Samples per byte

                let in_width = (png.header.width as usize / samples) + 1; // +1 for filter type
                let out_width = (png.header.width as usize) + 1;

                let mut out = vec![0; out_width * png.header.height as usize];

                png.iterate_rows(|row, input|{
                    let output = &mut out[out_width * row + 1 .. out_width * (row + 1)];

                    Unpack::unpack(input, output, b as usize, samples);
                });

                png.header.colour.bit_depth = BitDepth::B8;
                png.data = out;
            }
        }

        Ok(())
    }
}

pub struct Recolor(Palette<'static>);

impl Recolor {
    pub fn new<'a>(p: &'a Palette<'a>) -> Self {
        Self(Cow::Owned(p.clone().into_owned()))
    }
}

impl InputTransform for Recolor {
    fn transform(&self, png: &mut Png) -> Result<(), TransformError> {
        let palette_source = png.palette.as_ref().ok_or(TransformError)?;
        let palette_target = &self.0;

        let mut conversion = vec![0u8; palette_source.len()];

        for color_source in 0..palette_source.len() {
            for color_target in 0..palette_target.len() {
                if palette_source[color_source] == palette_target[color_target] {
                    conversion[color_source] = color_target as u8;
                }
            }
        }
        
        png.iterate_rows_mut(|_, buffer|{
            for b in buffer.iter_mut(){
                *b = conversion[*b as usize];
                if *b == 8 {
                    panic!("OWOWOWOWOWO");
                }
            }
        });

        png.palette = Some(self.0.clone().into_owned());

        Ok(())
    }
}