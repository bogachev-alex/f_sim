//! Поток кадров в визуализатор. Generic-параметр матча: в пакетных прогонах `NoopSink`,
//! компилятор удаляет вызовы целиком.

use crate::world::World;

pub trait FrameSink {
    fn frame(&mut self, tick: u32, world: &World);
}

pub struct NoopSink;

impl FrameSink for NoopSink {
    #[inline(always)]
    fn frame(&mut self, _tick: u32, _world: &World) {}
}
