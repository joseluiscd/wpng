use nom::{
    IResult,
    number::complete::be_u32,
    do_parse, named, take
};
use std::io::{Write, Result as IoResult};
use std::borrow::Cow;

pub trait Dump{
    fn dump<W: Write>(&self, w: W) -> IoResult<()>;
}

pub struct RawPng<'a>{

}

pub struct RawChunk<'a>{
    pub name: Cow<'a, [u8]>,
    pub data: Cow<'a, [u8]>,
    pub crc: Cow<'a, [u8]>
}

named!{parse_chunk <&[u8], RawChunk>,
    do_parse!(
        size: be_u32 >>
        name: take!(4) >>
        data: take!(size) >>
        crc: take!(4) >>
        (RawChunk{
            name: name.into(),
            data: data.into(),
            crc: crc.into()
        })
    )
}

impl <'a> RawChunk<'a> {
    pub fn parse(input: &'a[u8]) -> IResult<&'a[u8], Self> {
        parse_chunk(input)
    }
}

impl <'a> Dump for RawChunk<'a> {
    fn dump<W: Write>(&self, mut w: W) -> IoResult<()> {
        let size_bytes = (self.data.len() as u32).to_be_bytes();

        w.write_all(&size_bytes)?;
        w.write_all(&self.name)?;
        w.write_all(&self.data)?;
        w.write_all(&self.crc)
    }
}


#[cfg(test)]
mod tests {
    #[test]
    fn it_works() {
        assert_eq!(2 + 2, 4);
    }
}
