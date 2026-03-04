mod utils;
mod model;
mod processor;

use anyhow::Result;
use processor::AuraSeekEngine;
use utils::logger::Logger;

fn main() -> Result<()> {
    // initialize logger
    Logger::init("log/.log");
    
    // load engine
    let mut engine = AuraSeekEngine::new_default()?;
    engine.run_dir("input", "output")?;

    Ok(())
}
