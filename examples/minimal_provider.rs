//! Minimal Ember+ provider example.
//!
//! This example starts a TCP server on port 9000 and exposes a small cart-style
//! tree. Trigger events are printed to the console.

use rusty_embers::glow::GlowValue;
use rusty_embers::handler::Handler;
use rusty_embers::provider::Provider;
use rusty_embers::server::ProviderServer;
use rusty_embers::tree::CartTreeBuilder;
use rusty_embers::Error;
use std::sync::Arc;

struct ExampleHandler;

impl Handler for ExampleHandler {
    fn on_value_change(&self,
        path: &[u32],
        value: &GlowValue,
    ) -> Result<(), Error> {
        println!("value change at {:?}: {:?}", path, value);
        Ok(())
    }

    fn on_invoke(
        &self,
        path: &[u32],
        args: &[GlowValue],
    ) -> Result<Vec<GlowValue>, Error> {
        println!("invoke at {:?} with {:?}", path, args);
        Ok(Vec::new())
    }

    fn get_value(&self,
        _path: &[u32],
    ) -> Option<GlowValue> {
        None
    }
}

#[tokio::main]
async fn main() -> Result<(), Error> {
    tracing_subscriber::fmt::init();

    let mut builder = CartTreeBuilder::new("Example", 1);
    builder.add_cart(10, "Cart 1", "Intro");
    builder.add_cart(11, "Cart 2", "Sting");
    builder.add_global_stop(100);
    builder.add_global_now_playing(101);
    let tree = builder.build();

    let handler = Arc::new(ExampleHandler);
    let provider = Arc::new(Provider::new(tree, handler));
    let server = ProviderServer::new(provider);

    server.serve("0.0.0.0:9000").await
}
