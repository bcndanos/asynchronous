extern crate asynchronous;

use asynchronous::{Promise, EventLoop};
use std::sync::mpsc;

#[derive(Debug)]
enum Event {    
    Hello(String),
    Goodbye(u32)
}


fn function_test() {
    let event_loop_a = EventLoop::on_collect(move |event| {   
        match event {
            Event::Hello(t) => {
                println!("Hello {}", t);
                None
            },
            Event::Goodbye(v) => {
                println!("Farewell {}", v);
                Some(Event::Goodbye(v))
            },
        }        
    }).finish_in_ms(50);
    
    let event_loop_b = EventLoop::new();

    let e_l = event_loop_a.clone();
    let promise = Promise::<_,()>::new(move || {    
        &e_l.emit(Event::Hello("Inside Promise".to_string()));
        Ok(e_l)
    }).success(|e_l| {
        &e_l.emit(Event::Goodbye(1));
        Ok(e_l)
    });

    &event_loop_a.emit(Event::Hello("World".to_string()));

    &event_loop_b.emit("TestA");    

    let x = std::sync::Arc::new(std::sync::Mutex::new(0));
    event_loop_b.emit_until(move || {       
        let mut lock_x = x.lock().unwrap(); *lock_x += 1;
        if *lock_x <= 3 { Some("TestB") } else { None }
    });

    let (tx,rx) = mpsc::channel();    
    event_loop_b.emit_until(move || {
        match rx.recv() {
            Ok(v) => Some(v),
            Err(_) => None
        }
    });
    for _ in 0..3 { &tx.send("TestC"); }
    
    println!("Result 2: {:?}", event_loop_a.emit(Event::Goodbye(2)));
    let _ = promise.sync();
    println!("Result 3: {:?}", event_loop_a.emit(Event::Goodbye(3)));    
    println!("Result 4: {:?}", event_loop_a.emit(Event::Goodbye(4)));
    let event_loop_a = event_loop_a.finish();
    println!("Result 5: {:?}", event_loop_a.emit(Event::Goodbye(5)));
    println!("Result 6: {:?}", event_loop_a.emit(Event::Goodbye(6)));

    
    &event_loop_a.to_promise().then(|res| {
        let lock = res.lock().unwrap();
        println!("Event Loop a: {:?} ", *lock);
        event_loop_b.finish().to_promise().sync()
    }, |error| {
        println!("Error in loop a: {:?}", error);
        Err(())
    }).success(|res| {
        let lock = res.lock().unwrap();
        println!("Event Loop b: {:?} ", *lock);
        Ok(())
    }).sync();
    
}

fn main () {
    function_test();    
    std::thread::sleep_ms(500);
}