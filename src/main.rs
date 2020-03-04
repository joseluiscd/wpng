use wpng::raw::*;
use std::io::Read;
use std::fs::File;

fn main(){
    let mut k = Vec::new();
    let f = File::open("input.png").unwrap().read_to_end(&mut k);

    let (i, png) = RawPng::parse(&k).unwrap();
    println!("{:?}", i);
    println!("{:?}", png);

    let copy = File::create("miau.png").unwrap();
    //png.dump(copy);

}