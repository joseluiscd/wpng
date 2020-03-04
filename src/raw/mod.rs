use nom::{
    IResult,
    number::complete::be_u32,
    do_parse, named, take, tag,
    many_till, verify, map
};
use std::io::{Write, Result as IoResult};
use std::borrow::Cow;
use crc32fast::Hasher;
use std::convert::TryInto;


pub trait Dump{
    fn dump<W: Write>(&self, w: W) -> IoResult<()>;
}

pub const SIGNATURE: &[u8; 8] = &[0x89, b'P', b'N', b'G', b'\r', b'\n', 0x1A, b'\n'];
const IEND: &[u8; 4] = &[b'I', b'E', b'N', b'D'];

const IEND_CRC: u32 = 0xAE426082;
const IEND_CRC_BYTES: &[u8; 4] = unsafe { std::mem::transmute(&IEND_CRC) };

#[derive(Debug)]
pub struct RawChunk<'a>{
    pub name: [u8; 4],
    pub data: Cow<'a, [u8]>,
}

#[derive(Debug)]
pub struct RawPng<'a>(Vec<RawChunk<'a>>);

named!{parse_chunk <&[u8], RawChunk>,
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
            |(chunk, crc)|{
                let mut hasher = Hasher::new();
                hasher.update(&chunk.name);
                hasher.update(&chunk.data);
    
                let value = hasher.finalize();
                println!("MIAU: {}, {}", crc, value);
                *crc == value
            }
        ),
        |(chunk, _)| chunk
    )
}

named!{parse_end <&[u8], RawChunk>,

    do_parse!(
        tag!(&[0, 0, 0, 0]) >>
        tag!(IEND) >>
        tag!(IEND_CRC_BYTES) >>
        (RawChunk{
            name: *IEND,
            data: Cow::Borrowed(&[]),
        })
    )
}

named!{parse_png <&[u8], RawPng>,
    do_parse!(
        tag!(SIGNATURE) >> 
        chunks: many_till!(parse_chunk, parse_end) >>
        (RawPng(chunks.0))
    )
}

impl <'a> RawPng<'a> {
    pub fn parse(input: &'a[u8]) -> IResult<&'a[u8], Self> {
        parse_png(input)
    }
}

impl <'a> IntoIterator for &'a RawPng<'a> {
    type Item = &'a RawChunk<'a>;
    type IntoIter = std::slice::Iter<'a, RawChunk<'a>>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

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
        parse_chunk(input)
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
