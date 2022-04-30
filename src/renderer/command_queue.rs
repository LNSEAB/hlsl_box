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

    pub fn set_event(&self, event: &Event) -> anyhow::Result<()> {
        unsafe {
            self.fence
                .SetEventOnCompletion(self.value, event.handle())?;
            Ok(())
        }
    }
}

pub struct CommandQueue {
    queue: ID3D12CommandQueue,
    fence: ID3D12Fence,
    value: Cell<u64>,
}

impl CommandQueue {
    pub fn new(
        name: &str,
        device: &ID3D12Device,
        t: D3D12_COMMAND_LIST_TYPE,
    ) -> anyhow::Result<Self> {
        unsafe {
            let queue: ID3D12CommandQueue =
                device.CreateCommandQueue(&D3D12_COMMAND_QUEUE_DESC {
                    Type: t,
                    ..Default::default()
                })?;
            let fence: ID3D12Fence = device.CreateFence(0, D3D12_FENCE_FLAG_NONE)?;
            queue.SetName(format!("{}::queue", name))?;
            fence.SetName(format!("{}::fence", name))?;
            Ok(Self {
                queue,
                fence,
                value: Cell::new(1),
            })
        }
    }

    pub fn execute_command_lists(
        &self,
        cmd_lists: &[Option<ID3D12CommandList>],
    ) -> anyhow::Result<Signal> {
        unsafe {
            self.queue.ExecuteCommandLists(cmd_lists);
            self.signal()
        }
    }

    pub fn signal(&self) -> anyhow::Result<Signal> {
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

    pub fn wait(&self, signal: &Signal) -> anyhow::Result<()> {
        unsafe {
            self.queue.Wait(&signal.fence, signal.value)?;
        }
        Ok(())
    }

    pub fn handle(&self) -> &ID3D12CommandQueue {
        &self.queue
    }
}
