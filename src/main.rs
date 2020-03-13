use wpng::raw::*;
use std::io::Read;
use std::fs::File;
use wpng::*;
use std::convert::TryInto;
use wpng::transform::*;
use structopt::StructOpt;
use std::path::PathBuf;
use std::borrow::Cow;


#[derive(StructOpt)]
struct CommandArgs{
    /// Camera input file
    #[structopt(short, long, parse(from_os_str))]
    palette: PathBuf,

    #[structopt(short, long, parse(from_os_str))]
    images: Vec<PathBuf>,
}

fn main(){
    let args = CommandArgs::from_args();

    
    let recolor = {
        let mut k = Vec::new();
        let f = File::open(args.palette).unwrap().read_to_end(&mut k);

        let (i, png) = RawPng::parse(&k).unwrap();
        let mut png: Png = png.try_into().unwrap();
        
        wpng::transform::Recolor::new(&Cow::Owned(png.palette.unwrap()))
    };


    for image in args.images {
        let mut k = Vec::new();
        let f = File::open(&image).unwrap().read_to_end(&mut k);

        let (i, png) = RawPng::parse(&k).unwrap();
        let mut png: Png = png.try_into().unwrap();

        wpng::transform::Unpack.transform(&mut png).unwrap();
        recolor.transform(&mut png).unwrap();

        let r: RawPng = (&png).try_into().unwrap();
        
        r.dump(File::create(&image).unwrap()).unwrap();
    }
}