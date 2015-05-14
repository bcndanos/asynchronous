extern crate asynchronous;

use asynchronous::{Promise, EventLoop, Emit};
use std::sync::mpsc;

#[derive(Debug)]
enum Event {    
    Hello(String),
    Goodbye(u32)
}


fn function_test() {
    let event_loop_a = EventLoop::on_managed(move |event| {   
        match event {
            Event::Hello(t) => {
                println!("Hello {}", t);
                Emit::Continue
            },
            Event::Goodbye(v) => {
                println!("Farewell {}", v);
                Emit::Event(Event::Goodbye(v))
            },
        }        
    }).finish_in_ms(50);
    
    let event_loop_b = EventLoop::new();

    let event_handler = event_loop_a.get_handler();
    let promise = Promise::<_,()>::new(move || {    
        &event_handler.emit(Event::Hello("Inside Promise".to_string()));
        Ok(event_handler)
    }).success(|event_handler| {
        &event_handler.emit(Event::Goodbye(1));
        Ok(event_handler)
    });

    &event_loop_a.emit(Event::Hello("World".to_string()));

    &event_loop_b.emit("TestA");    

    let x = std::sync::Arc::new(std::sync::Mutex::new(0));
    event_loop_b.emit_until(move || {       
        let mut lock_x = x.lock().unwrap(); *lock_x += 1;
        if *lock_x <= 3 { Emit::Event("TestB") } else { Emit::Stop }
    });

    let (tx,rx) = mpsc::channel();    
    event_loop_b.emit_until(move || {
        match rx.recv() {
            Ok(v) => Emit::Event(v),
            Err(_) => Emit::Stop
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
        println!("Event Loop a: {:?} ", res);
        event_loop_b.finish_in_ms(200).to_promise().sync()
    }, |error| {
        println!("Error in loop a: {:?}", error);
        Err(())
    }).success(|res| {
        println!("Event Loop b: {:?} ", res);
        Ok(())
    }).sync();
    
}

fn main () {
    function_test();    
    std::thread::sleep_ms(500);
}