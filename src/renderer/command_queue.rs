use super::*;

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

    pub fn wait(&self) -> Result<(), Error> {
        if !self.is_completed() {
            let event = Event::new()?;
            self.set_event(&event)?;
            event.wait();
        }
        Ok(())
    }
}

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

    pub fn wait(&self, index: usize) {
        if let Some(signal) = self.signals.borrow_mut()[index].take() {
            if !signal.is_completed() {
                signal.set_event(&self.event).unwrap();
                self.event.wait();
            }
        }
    }

    pub fn wait_all(&self) {
        let mut signals = self.signals.borrow_mut();
        for signal in signals.iter_mut().flat_map(|s| s.take()) {
            if !signal.is_completed() {
                signal.set_event(&self.event).unwrap();
                self.event.wait();
            }
        }
    }

    pub fn last_frame_index(&self) -> Option<usize> {
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
    }
}

pub(super) struct CommandQueue<T> {
    queue: ID3D12CommandQueue,
    fence: ID3D12Fence,
    value: Cell<u64>,
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
                value: Cell::new(1),
                _t: std::marker::PhantomData,
            })
        }
    }

    pub fn execute_command_lists(
        &self,
        cmd_lists: &[Option<ID3D12CommandList>],
    ) -> Result<Signal, Error> {
        unsafe {
            self.queue.ExecuteCommandLists(cmd_lists);
            self.signal()
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
            let value = self.value.get();
            self.queue.Signal(&self.fence, value)?;
            self.value.set(value + 1);
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
