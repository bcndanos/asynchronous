extern crate asynchronous;

use std::sync::{Arc, Mutex};
use std::thread;
use asynchronous::*;

#[test]
fn promises() {     
    for x in 0..10 {
        let promise = Promise::<u32,&str>::new(move || { 
            match x {
                0 => Err("Division by zero"),
                _ => Ok(x * 2)
            }
        }).success(move |res| {
            assert_eq!(res, x * 2);
            Ok(res * 2) 
        }).fail(|error| {
            assert_eq!(error, "Division by zero");
            Err(error)
        }) ;

        let result = promise.sync();

        match x {
            0 => assert!(result.is_err()),
            _ => {
                assert!(result.is_ok());
                assert_eq!(result.unwrap(), x * 4);
            }
        }
    }
}

#[test]
fn promises_parallel() {
    let promise1 = Promise::<u32,&str>::new(|| {
        Ok(1u32)
    });
    let promise2 = Promise::<u32,&str>::new(|| {
        Ok(2u32)
    });
    let promise3 = Promise::<u32,&str>::new(|| {
        Ok(3u32)
    });
    let promise4 = Promise::<u32,&str>::new(|| {
        Err("Error")
    });
    let promise5 = Promise::<u32,&str>::new(|| {
        Ok(5u32)
    });        
    Promise::all(vec![promise1, promise2, promise3]).finally_sync(|res| {
        assert_eq!(res.unwrap(), vec![1u32,2u32,3u32]);    
    });
    Promise::all(vec![promise4, promise5]).finally_sync(|res| {        
        match res {
            Ok(_) => unreachable!(),
            Err(err) => {
                assert!(err[0].is_err());
                assert!(err[1].is_ok());        
            }
        }        
    });
}


#[test]
fn deferred_to_promise() {        
    Deferred::<u32,&str>::new(|| {                        
        Ok(88u32)
    }).to_promise().finally_sync(|r| {
        assert_eq!(r.unwrap(), 88u32);
    });        
}

#[test]
fn deferred_in_series() {        
    let st = Arc::new(Mutex::new(String::new()));

    let lock1 = st.clone();
    let d1 = Deferred::<u32, &str>::new(move ||{            
        thread::sleep_ms(200);            
        lock1.lock().unwrap().push_str("Def1");
        Ok(1u32)
    });

    let lock2 = st.clone();
    let d2 = Deferred::<u32, &str>::new(move || {
        thread::sleep_ms(100);
        lock2.lock().unwrap().push_str("Def2");            
        Ok(2u32)
    });

    let lock3 = st.clone();
    let d3 = Deferred::<u32, &str>::new(move ||{
        thread::sleep_ms(200);
        lock3.lock().unwrap().push_str("Def3");            
        Ok(3u32)
    });

    let d4 = Deferred::<u32, &str>::new(|| {
        Ok(4u32)
    }).chain(|res| {
        Ok(res.unwrap() * 3)
    });
    let d5 = Deferred::<u32, &str>::new(|| {
        Err("Error")            
    });
    let d6 = Deferred::<u32, &str>::new(|| {
        Ok(6u32)
    });

    let r = Deferred::vec_to_promise(vec![d1, d2, d3], ControlFlow::Series)
        .success(|res| {
            assert_eq!(vec![1u32,2u32, 3u32], res);
            Ok(0u32)
        }).sync();        
    assert_eq!(r, Ok(0u32));
    assert_eq!(*st.lock().unwrap(),"Def1Def2Def3");

    Deferred::vec_to_promise(vec![d4,d5,d6], ControlFlow::Series)
        .finally_sync(|res| {
            match res {
                Ok(r) => unreachable!("Res: {:?}", r),
                Err(errors) => {
                    assert_eq!(errors.len(), 2);
                    assert_eq!(errors[0], Ok(12u32));
                    assert_eq!(errors[1], Err("Error"));
                }
            }
            
        });
}

#[test]
fn deferred_in_parallel() {
    let st = Arc::new(Mutex::new(String::new()));

    let lock1 = st.clone();
    let d1 = Deferred::<u32, &str>::new(move ||{
        thread::sleep_ms(200);
        lock1.lock().unwrap().push_str("Def1");
        Ok(1u32)
    });
    let lock2 = st.clone();
    let d2 = Deferred::<u32, &str>::new(move || {
        thread::sleep_ms(300);
        lock2.lock().unwrap().push_str("Def2");
        Ok(2u32)
    });
    let lock3 = st.clone();
    let d3 = Deferred::<u32, &str>::new(move ||{
        thread::sleep_ms(50);
        lock3.lock().unwrap().push_str("Def3");
        Ok(3u32)
    });
    let d4 = Deferred::<u32, &str>::new(|| {
        Ok(4u32)
    });
    let d5 = Deferred::<u32, &str>::new(|| {
        Err("Error")            
    });
    let d6 = Deferred::<u32, &str>::new(|| {
        Ok(6u32)
    });

    let r = Deferred::vec_to_promise(vec![d1, d2, d3], ControlFlow::Parallel)
        .success(|res| {
            assert_eq!(vec![1u32,2u32, 3u32], res);
            Ok(0u32)
        }).sync();        
    assert_eq!(r, Ok(0u32));
    assert_eq!(*st.lock().unwrap(),"Def3Def1Def2");
    
    Deferred::vec_to_promise(vec![d4,d5,d6], ControlFlow::Parallel)
        .finally_sync(|res| {            
            match res {
                Ok(r) => unreachable!("Res: {:?}", r),
                Err(errors) => {
                    assert_eq!(errors.len(), 3);
                    assert_eq!(errors[0], Ok(4u32));
                    assert_eq!(errors[1], Err("Error"));
                    assert_eq!(errors[2], Ok(6u32));
                }
            }            
        });
}    

#[test]
fn deferred_in_parallel_limit() {
    let st = Arc::new(Mutex::new(String::new()));

    let lock1 = st.clone();
    let d1 = Deferred::<u32, &str>::new(move ||{
        thread::sleep_ms(150);
        lock1.lock().unwrap().push_str("Def1");
        Ok(1u32)
    });
    let lock2 = st.clone();
    let d2 = Deferred::<u32, &str>::new(move || {
        thread::sleep_ms(300);
        lock2.lock().unwrap().push_str("Def2");
        Ok(2u32)
    });
    let lock3 = st.clone();
    let d3 = Deferred::<u32, &str>::new(move ||{
        thread::sleep_ms(50);
        lock3.lock().unwrap().push_str("Def3");
        Ok(3u32)
    });
    let lock4 = st.clone();
    let d4 = Deferred::<u32, &str>::new(move || {
        thread::sleep_ms(200);
        lock4.lock().unwrap().push_str("Def4");
        Ok(4u32)
    });
    
    let d5 = Deferred::<u32, &str>::new(|| {    
        Ok(5u32)
    });
    let d6 = Deferred::<u32, &str>::new(|| {    
        Err("Error d")
    });

    let r = Deferred::vec_to_promise(vec![d1, d2, d3, d4], ControlFlow::ParallelLimit(2))
        .success(|res| {
            assert_eq!(vec![1u32,2u32, 3u32,4u32], res);
            Ok(0u32)
        }).sync();        
    assert_eq!(r, Ok(0u32));
    assert_eq!(*st.lock().unwrap(),"Def1Def3Def2Def4");
    
    Deferred::vec_to_promise(vec![d5,d6], ControlFlow::ParallelLimit(1))
        .finally_sync(|res| {
            match res {
                Ok(r) => unreachable!("Res: {:?}", r),
                Err(errors) => {
                    assert_eq!(errors.len(), 2);
                    assert_eq!(errors[0], Ok(5u32));
                    assert_eq!(errors[1], Err("Error d"));
                }
            }
        });
}        

#[test]
fn deferred_in_parallel_limit_cpus() {    
    let mut vec = Vec::new();
    for i in 1..5 {
        vec.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));         
    }
    Deferred::vec_to_promise(vec, ControlFlow::ParallelCPUS)
        .finally_sync(|res| {
            assert_eq!(res.unwrap(), vec![1u32, 2u32, 3u32, 4u32]);
        });
}

#[test]
fn deferred_first_wait() {
    let mut v = vec![];
    for i in 0..20 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(2, true, v, ControlFlow::Series)
        .finally_sync(|res| {                               
            assert_eq!(res.unwrap().len(), 2);                
        });

    let mut v = vec![];
    for i in 0..20 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(5, true, v, ControlFlow::ParallelLimit(3))
        .finally_sync(|res| {               
            let r = res.unwrap();
            assert!(r.len()>=5 && r.len()<=7);                
        });

    let mut v = vec![];
    for i in 0..20 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(5, true, v, ControlFlow::Parallel)
        .finally_sync(|res| {               
            assert_eq!(res.unwrap().len(), 20);                
        });

    let mut v = vec![];
    for i in 0..5 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    v.push(Deferred::<u32, &str>::new(move ||{ Err("Error in the middle") }));
    for i in 6..20 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(10, true, v, ControlFlow::Parallel)
        .finally_sync(|res| {                                           
            assert_eq!(res.unwrap().len(), 19);                   
        });  

    let mut v = vec![];
    for i in 0..5 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    for _ in 5..10 {
        v.push(Deferred::<u32, &str>::new(move ||{ Err("Error") }));
    }
    let _ = Deferred::first_to_promise(7, true, v, ControlFlow::ParallelLimit(3))
        .finally_sync(|res| {                                           
            assert_eq!(res.unwrap_err().len(), 10);  
        });  

    let mut v = vec![];
    for i in 0..5 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(7, true, v, ControlFlow::ParallelLimit(3))
        .finally_sync(|res| {                                           
            assert_eq!(res.unwrap_err().len(), 5);  
        });              
}                

#[test]
fn deferred_first_no_wait() {
    let mut v = vec![];
    for i in 0..20 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(2, false, v, ControlFlow::Series)
        .finally_sync(|res| {                               
            assert_eq!(res.unwrap().len(), 2);                
        });

    let mut v = vec![];
    for i in 0..20 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(5, false, v, ControlFlow::ParallelLimit(3))
        .finally_sync(|res| {               
            assert_eq!(res.unwrap().len(), 5);                
        });

    let mut v = vec![];
    for i in 0..20 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(5, false, v, ControlFlow::Parallel)
        .finally_sync(|res| {               
            assert_eq!(res.unwrap().len(), 5);                
        });

    let mut v = vec![];
    for i in 0..5 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    v.push(Deferred::<u32, &str>::new(move ||{ Err("Error in the middle") }));
    for i in 6..20 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(10, false, v, ControlFlow::ParallelLimit(3))
        .finally_sync(|res| {                                           
            assert_eq!(res.unwrap().len(), 10); 
        }); 

    let mut v = vec![];
    for i in 0..5 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    for _ in 5..10 {
        v.push(Deferred::<u32, &str>::new(move ||{ Err("Error") }));
    }
    let _ = Deferred::first_to_promise(7, false,  v, ControlFlow::ParallelLimit(3))
        .finally_sync(|res| {                                           
            assert_eq!(res.unwrap_err().len(), 10);  
        }); 

    let mut v = vec![];
    for i in 0..5 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(7, false, v, ControlFlow::ParallelLimit(3))
        .finally_sync(|res| {                                           
            assert_eq!(res.unwrap_err().len(), 5);  
        });                          
}                    

#[test]
fn deferred_chained() {
    let res = Deferred::<String, &str>::new(||{
        thread::sleep_ms(50);
        if true { Ok("first".to_string()) } else { Err("Nothing") }
    }).chain(|res| {  
        let mut v = res.unwrap();      
        assert_eq!(v, "first");
        thread::sleep_ms(50);
        if true { 
            v.push_str("second"); 
            Ok(v)
        } else { 
            Err("Nothing") 
        }
    }).to_promise().sync().unwrap();
    assert_eq!(res, "firstsecond");
}

#[test]
fn nested_promises() {
    let res = Promise::<_,&str>::new(|| {            
        // Do nothing
        Promise::new(|| {
            Promise::new(|| {
                Ok(4)
            }).success(|res| {
                Ok(res + 2)
            }).sync()
        }).success(|res| {
            Ok(res * 7)
        }).sync()
    }).success(|res| {
        Ok(res + 5)
    }).sync().unwrap();
    assert_eq!(res, 47);
}

#[test]
fn event_loop_1() {
    let event_loop = EventLoop::new();
    assert_eq!(event_loop.emit("EventA"), Ok(()));
    assert_eq!(event_loop.emit("EventB"), Ok(()));
    assert_eq!(event_loop.emit("EventC"), Ok(()));
    let res = event_loop.finish().to_promise().sync().unwrap();
    assert_eq!(res, vec!["EventA", "EventB", "EventC"]);
}

#[test]
fn event_loop_2() {
    let event_loop = EventLoop::new().finish_in_ms(50);
    assert_eq!(event_loop.emit("EventA"), Ok(()));
    assert_eq!(event_loop.emit("EventB"), Ok(()));
    thread::sleep_ms(100);
    assert_eq!(event_loop.emit("EventC"), Err("EventC"));
    let res = event_loop.finish().to_promise().sync().unwrap();
    assert_eq!(res, vec!["EventA", "EventB"]);
}    

#[test]
fn event_loop_3() {
    let event_loop = EventLoop::new();
    assert_eq!(event_loop.emit("EventA"), Ok(()));
    assert_eq!(event_loop.emit("EventB"), Ok(()));
    let event_loop = event_loop.finish();
    assert_eq!(event_loop.emit("EventC"), Err("EventC"));
    assert_eq!(event_loop.is_active(), false);
    let res = event_loop.to_promise().sync().unwrap();
    assert_eq!(res, vec!["EventA", "EventB"]);
}        

#[test]
fn event_loop_4() {
    let event_loop = EventLoop::new().finish_in_ms(450);
    assert_eq!(event_loop.emit("EventA"), Ok(()));        
    assert_eq!(event_loop.emit("EventB"), Ok(()));
    let x = Arc::new(Mutex::new(0));
    event_loop.emit_until(move || {
        thread::sleep_ms(100);
        let mut lock_x = x.lock().unwrap(); *lock_x += 1;
        if *lock_x == 2 { return Emit::Continue }
        if *lock_x <= 5 { Emit::Event("EventC") } else { Emit::Stop }
    });
    let res = event_loop.to_promise().sync().unwrap();
    assert_eq!(res, vec!["EventA", "EventB", "EventC", "EventC", "EventC"]);
}     

#[test]
fn event_loop_on_1() {
    let v = Arc::new(Mutex::new(Vec::<&str>::new()));
    let v_cloned = v.clone();
    let event_loop = EventLoop::on(move |event| {
        match event {
            "EventA" => { let mut v_lock = v_cloned.lock().unwrap(); v_lock.push("EventATreated"); },
            "EventB" => (),
            _ => { let mut v_lock = v_cloned.lock().unwrap(); v_lock.push("EventOtherTreated"); },
        }
    });
    assert_eq!(event_loop.emit("EventA"), Ok(()));
    assert_eq!(event_loop.emit("EventB"), Ok(()));
    assert_eq!(event_loop.emit("EventC"), Ok(()));
    let res = event_loop.finish().to_promise().sync().unwrap();
    assert_eq!(res.len(), 0);
}

#[test]
fn event_loop_on_2() {
    enum Event {
        Hello(String),
        Goodbye(String)
    }
    let v = Arc::new(Mutex::new(Vec::<String>::new()));
    let v_cloned = v.clone();
    let event_loop = EventLoop::on(move |event| {
        match event {
            Event::Hello(_) => (),
            Event::Goodbye(v) => { let mut v_lock = v_cloned.lock().unwrap(); v_lock.push(v); },
        }
    }).finish_in_ms(100);
    assert!(event_loop.emit(Event::Hello("World".to_string())).is_ok());
    assert!(event_loop.emit(Event::Goodbye("BCN".to_string())).is_ok());
    assert!(event_loop.emit(Event::Goodbye("MAD".to_string())).is_ok());
    let res = event_loop.to_promise().sync().unwrap();
    assert_eq!(res.len(), 0);
}    

#[test]
fn event_loop_on_managed_1() {
    let event_loop = EventLoop::on_managed(|event| {
        match event {
            "EventA" => Emit::Event("EventATreated"),
            "EventB" => Emit::Continue,
            "EventStop" => Emit::Stop,
            _ => Emit::Event("EventOtherTreated")
        }
    });
    assert_eq!(event_loop.emit("EventA"), Ok(()));
    assert_eq!(event_loop.emit("EventB"), Ok(()));
    assert_eq!(event_loop.emit("EventC"), Ok(()));
    assert_eq!(event_loop.emit("EventStop"), Ok(()));
    thread::sleep_ms(75);
    assert_eq!(event_loop.emit("EventE"), Err("EventE"));
    let res = event_loop.finish().to_promise().sync().unwrap();
    assert_eq!(res, vec!["EventATreated", "EventOtherTreated"]);
}

#[test]
fn event_loop_on_managed_2() {
    enum Event {
        Hello(String),
        Goodbye(String)
    }
    let event_loop = EventLoop::on_managed(|event| {
        match event {
            Event::Hello(_) => Emit::Continue,
            Event::Goodbye(v) => Emit::Event(Event::Goodbye(v)),
        }
    }).finish_in_ms(100);
    assert!(event_loop.emit(Event::Hello("World".to_string())).is_ok());
    assert!(event_loop.emit(Event::Goodbye("BCN".to_string())).is_ok());
    assert!(event_loop.emit(Event::Goodbye("MAD".to_string())).is_ok());
    let res = event_loop.to_promise().sync().unwrap();
    assert_eq!(res.len(), 2);
    match res[0] { Event::Goodbye(ref v) => assert_eq!(v, "BCN") , _ => panic!() };
    match res[1] { Event::Goodbye(ref v) => assert_eq!(v, "MAD") , _ => panic!() };
}        