use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

#[derive(Debug, Default)]
pub struct ProcessingTaskState {
    current: Mutex<Option<Arc<AtomicBool>>>,
}

impl ProcessingTaskState {
    pub fn begin(&self) -> Result<Arc<AtomicBool>, &'static str> {
        let mut current = self
            .current
            .lock()
            .expect("recognition task mutex poisoned");
        if current.is_some() {
            return Err("已有任务正在进行");
        }

        let token = Arc::new(AtomicBool::new(false));
        *current = Some(token.clone());
        Ok(token)
    }

    pub fn cancel(&self) -> bool {
        let current = self
            .current
            .lock()
            .expect("recognition task mutex poisoned");
        if let Some(token) = current.as_ref() {
            token.store(true, Ordering::Relaxed);
            true
        } else {
            false
        }
    }

    pub fn finish(&self, token: &Arc<AtomicBool>) {
        let mut current = self
            .current
            .lock()
            .expect("recognition task mutex poisoned");
        if current
            .as_ref()
            .is_some_and(|active| Arc::ptr_eq(active, token))
        {
            *current = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::Ordering;

    use super::ProcessingTaskState;

    #[test]
    fn permits_only_one_active_task() {
        let state = ProcessingTaskState::default();
        let first = state.begin().expect("first task");

        assert_eq!(
            state.begin().expect_err("second task should fail"),
            "已有任务正在进行"
        );

        state.finish(&first);
        assert!(state.begin().is_ok());
    }

    #[test]
    fn cancellation_marks_the_active_task() {
        let state = ProcessingTaskState::default();
        let task = state.begin().expect("task");

        assert!(state.cancel());
        assert!(task.load(Ordering::Relaxed));

        state.finish(&task);
        assert!(!state.cancel());
    }

    #[test]
    fn finishing_an_old_task_does_not_clear_a_newer_task() {
        let state = ProcessingTaskState::default();
        let old = state.begin().expect("old task");
        state.finish(&old);
        let current = state.begin().expect("current task");

        state.finish(&old);

        assert!(state.cancel());
        assert!(current.load(Ordering::Relaxed));
    }
}
