use nom::{
    IResult,
    Err as NomErr,
    error::ErrorKind,
    number::streaming::be_u32,
    number::streaming::be_u8,
    do_parse, named, take, tag,
    many_till, verify, map, tuple, map_opt,
    eof, alt, many1, complete
};
use std::io::{Write, Result as IoResult};
use std::borrow::Cow;
use crc32fast::Hasher;
use std::convert::TryInto;
use std::borrow::Borrow;
use flate2::Decompress;
use std::io::Read;

pub trait Dump{
    fn dump<W: Write>(&self, w: W) -> IoResult<()>;
}

pub const SIGNATURE: &[u8; 8] = &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
pub const IEND: &[u8; 4] = &b"IEND";
pub const IHDR: &[u8; 4] = &b"IHDR";
pub const IDAT: &[u8; 4] = &b"IDAT";
pub const PLTE: &[u8; 4] = &b"PLTE";

const IEND_CRC: &[u8; 4] = &[0xAE, 0x42, 0x60, 0x82];
#[derive(Debug, Clone)]
pub struct RawChunk<'a>{
    pub name: [u8; 4],
    pub data: Cow<'a, [u8]>
}

#[derive(Debug)]
pub enum Chunk<'a>{
    Palette(Palette<'a>),
    Data(Cow<'a, [u8]>),
    Other(RawChunk<'a>)
}

#[derive(Debug, Copy, Clone)]
#[repr(u8)]
pub enum ColourType {
    GreyScale = 0,
    TrueColour = 2,
    IndexedColour = 3,
    GreyScaleAlpha = 4,
    TrueColourAlpha = 6
}

#[derive(Debug, Copy, Clone)]
#[repr(u8)]
pub enum BitDepth {
    B2 = 2,
    B4 = 4,
    B8 = 8,
    B16 = 16,
}

#[derive(Debug, Copy, Clone)]
pub struct Colour {
    pub bit_depth: BitDepth,
    pub t: ColourType
}

#[derive(Debug, Copy, Clone)]
#[repr(u8)]
pub enum InterlaceMethod {
    NoInterlace = 0,
    Adam7 = 1,
    Error = 255
}

impl InterlaceMethod {
    fn from_u8(i: u8) -> Self {
        match i {
            0 => InterlaceMethod::NoInterlace,
            1 => InterlaceMethod::Adam7,
            _ => InterlaceMethod::Error
        }
    }
}

impl ColourType {
    fn from_u8(i: u8) -> Option<Self> {
        match i {
            0 => Some(ColourType::GreyScale),
            2 => Some(ColourType::TrueColour),
            3 => Some(ColourType::IndexedColour),
            4 => Some(ColourType::GreyScaleAlpha),
            6 => Some(ColourType::TrueColourAlpha),
            _ => None
        }
    }
}

impl BitDepth {
    fn from_u8(i: u8) -> Option<Self> {
        match i {
            2 => Some(BitDepth::B2),
            4 => Some(BitDepth::B4),
            8 => Some(BitDepth::B8),
            16 => Some(BitDepth::B16),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Header {
    pub width: u32,
    pub height: u32,
    pub colour: Colour,
    pub filter_method: u8,
    pub interlace: InterlaceMethod
}

pub type Palette<'a> = Cow<'a, [[u8;3]]>;


#[derive(Debug)]
pub struct RawPng<'a>(pub Cow<'a, Header>, pub Vec<Chunk<'a>>);

named!{parse_colour_type <&[u8], Colour>,
    map_opt!(
        tuple!(be_u8, be_u8),
        |(bit_depth, colour_type)| Colour::from_u8(bit_depth, colour_type)
    )
}

named!{parse_raw_chunk <&[u8], RawChunk>,
    map!(
        verify!(
            do_parse!(
                size: be_u32 >>
                name: take!(4) >>
                data: take!(size) >>
                crc: be_u32 >>
                (
                    (RawChunk {
                        name: name.try_into().unwrap(),
                        data: data.into(),
                    }, crc)
                )
            ),
            |(chunk, crc)| chunk.verify_crc32(*crc)
        ),
        |(chunk, _)| chunk
    )
}


fn parse_chunk_with<'a, T: 'a, F>(input: &'a [u8], f: F) -> IResult<&'a [u8], T>
    where F: 'static + Fn(RawChunk<'a>) -> Option<T>
{
    let (input, chunk) = parse_raw_chunk(input)?;
    let chunk = f(chunk).ok_or(NomErr::Error((input, ErrorKind::Tag)))?;

    Ok((input, chunk))
}

macro_rules! parse_chunk_data {
    (fn $name:ident<$lt:lifetime>($tag:expr, $input:ident) -> Option<$ret:ty> $parse:block) => {
        fn $name<$lt>(r: RawChunk<$lt>) -> Option<$ret> {
            if &r.name == $tag {
                let $input = r.data;
                $parse
            } else {
                None
            }
        }
    };
}

parse_chunk_data!{
    fn parse_chunk_ihdr<'a>(IHDR, input) -> Option<Header> {
        do_parse!(&*input,
            width: be_u32 >>
            height: be_u32 >>
            colour: parse_colour_type >>
            tag!(&[0]) >> // Compression
            filter_method: be_u8 >>
            interlace: be_u8 >>
            eof!() >>
            (Header{
                width, height,
                colour,
                filter_method,
                interlace: InterlaceMethod::from_u8(interlace)
            })
        ).map(|(_, a)| a).ok()
    }
}

parse_chunk_data!{
    fn parse_chunk_plte<'a>(PLTE, input) -> Option<Palette> {
        let l = input.len();
        
        if l % 3 == 0 {
            let k: &'a [[u8; 3]] = unsafe {
                std::slice::from_raw_parts(input.as_ptr() as *const _, l / 3)
            };

            Some(Cow::Borrowed(k))
        } else {
            None
        }

    }
}

parse_chunk_data!{
    fn parse_chunk_idat<'a>(IDAT, input) -> Option<Cow<[u8]>> {
        Some(input)
    }
}

named!{parse_end <&[u8], ()>,
    do_parse!(
        tag!(&[0, 0, 0, 0]) >>
        tag!(IEND) >>
        tag!(IEND_CRC) >>
        ()
    )
}

fn parse_ihdr(input: &[u8]) -> IResult<&[u8], Header> {
    parse_chunk_with(input, parse_chunk_ihdr)
}

fn parse_plte<'a>(input: &'a [u8]) -> IResult<&'a [u8], Palette<'a>> {
    parse_chunk_with(input, parse_chunk_plte)
}

fn parse_idat<'a>(input: &'a [u8]) -> IResult<&'a [u8], Cow<'a, [u8]>> {
    parse_chunk_with(input, parse_chunk_idat)
}

fn parse_chunk<'a>(input: &'a [u8]) -> IResult<&'a [u8], Chunk<'a>> {
    alt!(input, 
        map!(parse_plte, |c| Chunk::Palette(c)) |
        map!(parse_idat, |c| Chunk::Data(c)) |
        map!(parse_raw_chunk, |c| Chunk::Other(c))
    )
}

fn parse_png(input: &[u8]) -> IResult<&[u8], RawPng> {
    do_parse!(input,
        tag!(SIGNATURE) >> 
        header: parse_ihdr >>
        chunks: map!(many_till!(parse_chunk, parse_end), |(v, _)| v) >>
        (RawPng(Cow::Owned(header), chunks))
    )
}

/*named!{parse_png <&[u8], RawPng>,
    do_parse!(
        chunks: many_till!(parse_raw_chunk, parse_end) >>
        t: alt!(
            parse_header | parse_raw_chunk
        ) >>
        (RawPng(header, chunks.0))
    )
}*/

impl Colour {
    fn from_u8(bit_depth: u8, colour_type: u8) -> Option<Self> {
        // Todo: check things
        Some(Self {
            bit_depth: BitDepth::from_u8(bit_depth)?,
            t: ColourType::from_u8(colour_type)?
        })
    }
}

impl <'a> RawPng<'a> {
    pub fn parse(input: &'a[u8]) -> IResult<&'a[u8], Self> {
        parse_png(input)
    }
}

/*impl <'a> IntoIterator for &'a RawPng<'a> {
    type Item = &'a RawChunk<'a>;
    type IntoIter = std::slice::Iter<'a, RawChunk<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.1.iter()
    }
}*/

impl From<&Header> for RawChunk<'static> {
    fn from(h: &Header) -> Self {
        let mut w = Vec::<u8>::new();
        w.extend(&h.width.to_be_bytes());
        w.extend(&h.height.to_be_bytes());
        w.extend(&[
            h.colour.bit_depth as u8,
            h.colour.t as u8,
            0,
            h.filter_method,
            h.interlace as u8
        ]);

        RawChunk{
            name: *IHDR,
            data: Cow::Owned(w)
        }
    }
}

impl <'a> From<&'a Chunk<'a>> for RawChunk<'a> {
    fn from(c: &'a Chunk) -> Self {
        match c {
            Chunk::Palette(p) => p.into(),
            Chunk::Data(data) => RawChunk{ name: *IDAT, data: data.clone() },
            Chunk::Other(raw) => raw.clone(),
        }
    }
}

impl <'a> From<&'a Palette<'a>> for RawChunk<'a> {
    fn from(p: &'a Palette<'a>) -> Self {
        let d = &*p;
        let k: &[u8] = unsafe {
            std::slice::from_raw_parts(d.as_ptr() as *const _, d.len() * 3)
        };

        RawChunk{
            name: *PLTE,
            data: Cow::Borrowed(k)
        }
    }
}

impl <'a> Dump for Chunk<'a> {
    fn dump<W: Write>(&self, w: W) -> IoResult<()> {
        RawChunk::from(self).dump(w)
    }
}

impl <'a> Dump for RawPng<'a> {
    fn dump<W: Write>(&self, mut w: W) -> IoResult<()> {
        
        w.write_all(SIGNATURE)?;
        RawChunk::from(&*self.0).dump(&mut w)?;


        for chunk in self.1.iter() {
            chunk.dump(&mut w)?;
        }

        RawChunk::end().dump(w)
    }
}

impl <'a> RawChunk<'a> {
    pub fn parse(input: &'a[u8]) -> IResult<&'a[u8], Self> {
        parse_raw_chunk(input)
    }

    pub fn end() -> Self {
        Self {
            name: *IEND,
            data: Cow::Borrowed(&[]),
        }
    }

    pub fn is_end(&self) -> bool {
        self.name == *IEND
    }

    pub fn verify_crc32(&self, crc: u32) -> bool {
        let mut hasher = Hasher::new();
        hasher.update(&self.name);
        hasher.update(&*self.data);

        let value = hasher.finalize();
        crc == value
    }
}

impl <'a> Dump for RawChunk<'a> {
    fn dump<W: Write>(&self, mut w: W) -> IoResult<()> {
        let size_bytes = (self.data.len() as u32).to_be_bytes();

        let mut hasher = Hasher::new();
        hasher.update(&self.name);
        hasher.update(&self.data);
        let crc = hasher.finalize().to_be_bytes();

        w.write_all(&size_bytes)?;
        w.write_all(&self.name)?;
        w.write_all(&self.data)?;
        w.write_all(&crc)
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
