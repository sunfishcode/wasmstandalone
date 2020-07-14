//! Small example of how to serialize compiled wasm module to the disk,
//! and then instantiate it from the compilation artifacts.

// You can execute this example with `cargo run --example serialize`

use anyhow::Result;
use std::fs::{self, File};
use std::io::{Seek, SeekFrom};
use wasmtime::*;

fn serialize() -> Result<File> {
    // Configure the initial compilation environment, creating the global
    // `Store` structure. Note that you can also tweak configuration settings
    // with a `Config` and an `Engine` if desired.
    println!("Initializing...");
    let engine = Engine::default();

    // Compile the wasm binary into an in-memory instance of a `Module`.
    println!("Compiling module...");
    let wasm = fs::read("examples/hello.wat")?;
    let mut file = tempfile::tempfile()?;
    compile_and_serialize(&engine, wasm, file.try_clone()?)?;

    file.seek(SeekFrom::Start(0))?;

    println!("Serialized.");
    Ok(file)
}

fn deserialize(file: File) -> Result<()> {
    // Configure the initial compilation environment, creating the global
    // `Store` structure. Note that you can also tweak configuration settings
    // with a `Config` and an `Engine` if desired.
    println!("Initializing...");
    let store = Store::default();

    // Compile the wasm binary into an in-memory instance of a `Module`.
    println!("Compiling module...");
    let module = unsafe { Module::deserialize(store.engine(), file)? };

    // Here we handle the imports of the module, which in this case is our
    // `HelloCallback` type and its associated implementation of `Callback.
    println!("Creating callback...");
    let hello_func = Func::wrap(&store, || {
        println!("Calling back...");
        println!("> Hello World!");
    });

    // Once we've got that all set up we can then move to the instantiation
    // phase, pairing together a compiled module as well as a set of imports.
    // Note that this is where the wasm `start` function, if any, would run.
    println!("Instantiating module...");
    let imports = [hello_func.into()];
    let instance = Instance::new(&store, &module, &imports)?;

    // Next we poke around a bit to extract the `run` function from the module.
    println!("Extracting export...");
    let run = instance
        .get_func("run")
        .ok_or(anyhow::format_err!("failed to find `run` function export"))?
        .get0::<()>()?;

    // And last but not least we can call it!
    println!("Calling export...");
    run()?;

    println!("Done.");
    Ok(())
}

fn main() -> Result<()> {
    let file = serialize()?;
    deserialize(file)
}