mod ns16550a;

use crate::board::CharDeviceImpl;
use alloc::sync::Arc;
use lazy_static::*;
pub use ns16550a::NS16550a;

pub trait CharDevice {
    fn init(&self);
    fn read(&self) -> u8;
    fn try_read(&self) -> Option<u8>;
    fn has_input(&self) -> bool;
    fn write(&self, ch: u8);
    fn handle_irq(&self);
}

lazy_static! {
    pub static ref UART: Arc<CharDeviceImpl> =
        Arc::new(CharDeviceImpl::new(crate::board::uart_base()));
}
