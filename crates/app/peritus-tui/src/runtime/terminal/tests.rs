use super::*;

#[tokio::test]
async fn backpressure_retains_input_until_the_ui_can_receive_it() {
    let (sender, mut receiver) = mpsc::channel(1);
    let first = Event::Resize(80, 24);
    let next = Event::Resize(81, 25);
    sender.try_send(first.clone()).unwrap();
    let expected = next.clone();
    let worker = thread::spawn(move || forward_event(&sender, next, &AtomicBool::new(false)));
    assert_eq!(receiver.recv().await, Some(first));
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(1), receiver.recv()).await.unwrap(),
        Some(expected)
    );
    assert!(worker.join().unwrap());
}

#[test]
fn stopping_a_full_input_queue_never_waits_for_the_ui_to_drain_it() {
    for explicit_stop in [true, false] {
        let (sender, receiver) = mpsc::channel(1);
        sender.try_send(Event::Resize(80, 24)).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let worker_stop = Arc::clone(&stop);
        let (started, waiting) = std::sync::mpsc::channel();
        let worker = thread::spawn(move || {
            started.send(()).unwrap();
            let _ = forward_event(&sender, Event::Resize(81, 25), &worker_stop);
        });
        let mut pump = InputPump { stop, thread: Some(worker) };
        waiting.recv_timeout(Duration::from_secs(1)).unwrap();
        // Rescue a blocking implementation so the regression reports a failure, not a hang.
        let (done, rescued) = std::sync::mpsc::channel();
        let watchdog = thread::spawn(move || {
            let timed_out = rescued.recv_timeout(Duration::from_secs(1)).is_err();
            drop(receiver);
            timed_out
        });
        if explicit_stop {
            pump.stop().unwrap();
        } else {
            drop(pump);
        }
        let _ = done.send(());
        assert!(!watchdog.join().unwrap(), "input shutdown needed a queue-drain rescue");
    }
}
