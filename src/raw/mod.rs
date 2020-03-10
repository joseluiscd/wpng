use nom::{
    IResult,
    Err as NomErr,
    error::ErrorKind,
    number::streaming::be_u32,
    number::streaming::be_u8,
    do_parse, named, take, tag,
    many_till, verify, map, tuple,
    eof, alt, many1
};
use std::io::{Write, Result as IoResult};
use std::borrow::Cow;
use crc32fast::Hasher;
use std::convert::TryInto;
use std::borrow::Borrow;


pub trait Dump{
    fn dump<W: Write>(&self, w: W) -> IoResult<()>;
}

pub const SIGNATURE: &[u8; 8] = &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
const IEND: &[u8; 4] = &b"IEND";
const IHDR: &[u8; 4] = &b"IEND";

const IEND_CRC: &[u8; 4] = &[0xAE, 0x42, 0x60, 0x82];
#[derive(Debug)]
pub struct RawChunk<'a>{
    pub name: [u8; 4],
    pub data: &'a [u8],
}

#[derive(Debug)]
pub enum Chunk<'a>{
    Palette(Palette<'a>),
    Data,
    Other(RawChunk<'a>)
}

#[derive(Debug)]
#[repr(u8)]
pub enum ColourType {
    GreyScale = 0,
    TrueColour = 2,
    IndexedColour = 3,
    GreyScaleAlpha = 4,
    TrueColourAlpha = 6
}

#[derive(Debug)]
pub struct Colour {
    bit_depth: u8,
    t: ColourType
}

#[derive(Debug)]
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

#[derive(Debug)]
pub struct Header {
    width: u32,
    height: u32,
    colour: Colour,
    filter_method: u8,
    interlace: InterlaceMethod
}

#[derive(Debug)]
pub struct Palette<'a> {
    d: Cow<'a, [[u8;3]]>
}

#[derive(Debug)]
pub struct RawPng<'a>(Header, Vec<Chunk<'a>>);

named!{parse_colour_type <&[u8], Colour>,
    map!(
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
    where F: 'static + Fn(RawChunk<'a>) -> IResult<&'a [u8], T>
{
    let (input, chunk) = parse_raw_chunk(input)?;
    let (k, chunk) = f(chunk)?;
    eof!(k,)?;

    Ok((input, chunk))
}

macro_rules! parse_chunk_data {
    (fn $name:ident($tag:expr, $input:ident) -> $ret:ty $parse:block) => {
        fn $name(r: RawChunk) -> IResult<&[u8], $ret> {
            if &r.name == $tag {
                let $input = r.data;
                $parse
            } else {
                Err(nom::Err::Error((r.data, nom::error::ErrorKind::Tag)))
            }
        }
    };
}

parse_chunk_data!{
    fn parse_chunk_ihdr(IHDR, input) -> Header {
        do_parse!(input,
            width: be_u32 >>
            height: be_u32 >>
            colour: parse_colour_type >>
            tag!(&[0]) >> // Compression
            filter_method: be_u8 >>
            interlace: be_u8 >>
            (Header{
                width, height,
                colour,
                filter_method,
                interlace: InterlaceMethod::from_u8(interlace)
            })
        )
    }
}

parse_chunk_data!{
    fn parse_chunk_plte(IHDR, input) -> Palette {
        let l = input.len();
        
        if l % 3 == 0 {
            let k: &[[u8; 3]] = unsafe {
                std::slice::from_raw_parts(input.as_ptr() as *const _, l / 3)
            };

            Ok((&[], Palette{
                d: Cow::Borrowed(k)
            }))
        } else {
            Err(NomErr::Failure((input, ErrorKind::Count)))
        }

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

fn parse_chunk<'a>(input: &'a [u8]) -> IResult<&'a [u8], Chunk<'a>> {
    alt!(input, 
        map!(parse_plte, |c| Chunk::Palette(c)) |
        map!(parse_raw_chunk, |c| Chunk::Other(c))
    )
}

/*fn parse_png_part<'a>(input: &'a[u8], p: RawPng<'a>) -> IResult<&'a[u8], RawPng<'a>> {
    match parse_end(input){
        Ok((input, _)) => Ok((input, p)),
        Err(_) => {
            alt!(input,
                parse_header!()
            )

            Ok((input, p))
        }
    }
}*/

fn parse_png(input: &[u8]) -> IResult<&[u8], RawPng> {
    do_parse!(input,
        tag!(SIGNATURE) >> 
        header: parse_ihdr >>
        chunks: map!(many_till!(parse_chunk, parse_end), |(v, _)| v) >>
        (RawPng(header, chunks))
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
    fn from_u8(bit_depth: u8, color_type: u8) -> Self {
        unimplemented!()
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

impl <'a, T> Dump for T
    where for<'b> &'b T: IntoIterator<Item=&'a RawChunk<'a>>
{
    fn dump<W: Write>(&self, mut w: W) -> IoResult<()> {
        w.write_all(SIGNATURE)?;
        for chunk in self {
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
            data: &[],
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
