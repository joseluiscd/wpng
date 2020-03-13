pub mod raw;
pub mod transform;

use raw::{
    RawPng, Header, Chunk, Palette, RawChunk
};
use std::borrow::Cow;
use flate2::bufread::{
    ZlibDecoder,
    ZlibEncoder,
};
use std::convert::TryFrom;
use std::path::Path;

pub type Scanline<'a> = Cow<'a, [u8]>;

#[derive(Debug)]
pub struct Png {
    pub header: Header,
    pub palette: Option<Vec<[u8; 3]>>,
    pub data: Vec<u8>,
}


impl Png {
    pub fn open(p: impl AsRef<Path>) -> Result<Self, std::io::Error> {
        use std::io::Read;
        use std::fs::File;
        use std::convert::TryInto;

        let mut buffer = Vec::new();

        let f = File::open(p.as_ref())?.read_to_end(&mut buffer);

        let (i, png) = RawPng::parse(&buffer).map_err(|_| std::io::Error::from(std::io::ErrorKind::Other))?;
        let png: Png = png.try_into()?;

        Ok(png)
    }

    pub fn iterate_rows<F>(&self, mut f: F)
        where F: (FnMut(usize, &[u8])->()) 
    {
        let bitdepth = self.header.colour.bit_depth as usize;
        let samples_per16 = 16 / bitdepth as usize; // Samples per 2bytes

        let in_width = (self.header.width as usize / samples_per16 / 2) + 1; // +1 for filter type
        for row in 0..self.header.height as usize {

            let input = &self.data[in_width * row + 1 .. in_width * (row + 1)];

            f(row, input)
        }
    }

    pub fn iterate_rows_mut<F>(&mut self, mut f: F)
        where F: (FnMut(usize, &mut [u8])->()) 
    {
        let bitdepth = self.header.colour.bit_depth as usize;
        let samples_per16 = 16 / bitdepth as usize; // Samples per 2bytes

        let in_width = (self.header.width as usize / samples_per16 / 2) + 1; // +1 for filter type
        for row in 0..self.header.height as usize {
            let buffer = &mut self.data[in_width * row + 1 .. in_width * (row + 1)];

            f(row, buffer)
        }
    }

    pub fn extract_pixels(&self) -> Vec<u8> {
        let mut ret = Vec::<u8>::new();

        self.iterate_rows(|row, buffer|{
            ret.extend(buffer);
        });

        ret
    }
}
impl <'a> TryFrom<RawPng<'a>> for Png {
    type Error = std::io::Error;

    fn try_from(raw: RawPng<'a>) -> Result<Self, Self::Error> {
        use std::io::Read;

        let RawPng(header, chunks) = raw;
        let mut raw_data = Vec::new();
        let mut data = Vec::new();
        let mut palette = None;

        for chunk in chunks {
            match chunk {
                Chunk::Palette(p) => {
                    palette.replace(p.into_owned());
                },
                Chunk::Data(d) => {
                    raw_data.extend(&*d);
                },
                _ => {}
            }
        }

        let mut d = ZlibDecoder::new(&*raw_data);
        d.read_to_end(&mut data)?;
        
        Ok(Png{
            header: header.into_owned(),
            palette,
            data
        })
    }
}

impl <'a> TryFrom<&'a Png> for RawPng<'a> {
    type Error = std::io::Error;

    fn try_from(p: &'a Png) -> Result<Self, Self::Error> {
        use std::io::Read;

        let Png{ header, palette, data } = p;

        let mut chunks = Vec::<Chunk>::new();

        if let Some(palette) = palette {
            chunks.push(Chunk::Palette(Cow::from(palette)));
        }

        let mut r = ZlibEncoder::new(&**data, flate2::Compression::default());
        let mut buf = vec![0; 4096];

        loop {
            let b = r.read(&mut buf)?;
            if b == 0 {
                break
            }
            
            chunks.push(Chunk::Data(Cow::Owned(buf[0..b].to_owned())));
        }

        Ok(RawPng(Cow::Borrowed(header), chunks))
    }

}