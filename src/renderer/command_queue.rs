use super::*;
use std::sync::atomic::{self, AtomicU64};

#[derive(Clone)]
pub struct Signal {
    fence: ID3D12Fence,
    value: u64,
}

impl Signal {
    pub fn is_completed(&self) -> bool {
        unsafe { self.fence.GetCompletedValue() >= self.value }
    }

    fn set_event(&self, event: &Event) -> Result<(), Error> {
        unsafe {
            self.fence
                .SetEventOnCompletion(self.value, event.handle())?;
            Ok(())
        }
    }

    pub async fn wait(&self) -> Result<(), Error> {
        if !self.is_completed() {
            let event = Event::new()?;
            self.set_event(&event)?;
            event.wait().await;
        }
        Ok(())
    }
}

unsafe impl Send for Signal {}
unsafe impl Sync for Signal {}

pub struct Signals {
    signals: RefCell<Vec<Option<Signal>>>,
    event: Event,
}

impl Signals {
    pub fn new(n: usize) -> Self {
        Self {
            signals: RefCell::new(vec![None; n]),
            event: Event::new().unwrap(),
        }
    }

    pub fn set(&self, index: usize, signal: Signal) {
        self.signals.borrow_mut()[index] = Some(signal);
    }

    pub async fn wait(&self, index: usize) {
        let signal = self.signals.borrow_mut()[index].take();
        if let Some(signal) = signal {
            if !signal.is_completed() {
                signal.set_event(&self.event).unwrap();
                self.event.wait().await;
            }
        }
    }

    pub async fn wait_all(&self) {
        let signals = self
            .signals
            .borrow_mut()
            .iter_mut()
            .flat_map(|s| s.take())
            .collect::<Vec<_>>();
        for signal in signals {
            if !signal.is_completed() {
                signal.set_event(&self.event).unwrap();
                self.event.wait().await;
            }
        }
    }

    pub fn last_frame(&self) -> Option<(usize, Signal)> {
        self.signals
            .borrow()
            .iter()
            .enumerate()
            .fold((None, 0), |(r, value), (index, signal)| {
                signal
                    .as_ref()
                    .and_then(|s| (s.value > value).then(|| (Some(index), s.value)))
                    .unwrap_or((r, value))
            })
            .0
            .map(|i| (i, self.signals.borrow()[i].as_ref().unwrap().clone()))
    }
}

pub(super) struct CommandQueue<T> {
    queue: ID3D12CommandQueue,
    fence: ID3D12Fence,
    value: AtomicU64,
    _t: std::marker::PhantomData<T>,
}

impl<T> CommandQueue<T>
where
    T: CommandList,
{
    pub fn new(name: &str, device: &ID3D12Device) -> Result<Self, Error> {
        unsafe {
            let queue: ID3D12CommandQueue =
                device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                    Type: T::LIST_TYPE,
                    ..Default::default()
                })?;
            let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE)?;
            queue.SetName(format!("{}::queue", name))?;
            fence.SetName(format!("{}::fence", name))?;
            Ok(Self {
                queue,
                fence,
                value: AtomicU64::new(1),
                _t: std::marker::PhantomData,
            })
        }
    }

    pub fn execute<const N: usize>(&self, cmd_lists: [&T; N]) -> Result<Signal, Error> {
        unsafe {
            let lists = cmd_lists.map(|l| Some(l.handle()));
            self.queue.ExecuteCommandLists(&lists);
            self.signal()
        }
    }

    pub fn signal(&self) -> Result<Signal, Error> {
        unsafe {
            let value = self.value.load(atomic::Ordering::SeqCst);
            self.queue.Signal(&self.fence, value)?;
            self.value.store(value + 1, atomic::Ordering::SeqCst);
            Ok(Signal {
                fence: self.fence.clone(),
                value,
            })
        }
    }

    pub fn wait(&self, signal: &Signal) -> Result<(), Error> {
        unsafe {
            self.queue.Wait(&signal.fence, signal.value)?;
        }
        Ok(())
    }

    pub fn handle(&self) -> &ID3D12CommandQueue {
        &self.queue
    }
}
