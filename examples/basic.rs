extern crate asynchronous;

use asynchronous::{Promise, Deferred, ControlFlow};
use std::thread;

fn promise_mock(a:u32,b:u32) -> Promise<f64,String> {
	Promise::new(move || {
		thread::sleep_ms(250); // Simulate async work
		match b {
			0u32 => Err("Division by zero".to_string()),
			_ => Ok(a as f64 / b as f64)
		}
	})
}

fn main() {

	println!("Starting Example!");

    let mut v = vec![];
    for i in 0..5 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    v.push(Deferred::<u32, &str>::new(move ||{ Err("Error in the middle") }));
    for i in 6..20 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(7, false, v, ControlFlow::ParallelLimit(3))
        .finally_sync(|res| {                                           
        	println!("RES: {:?}", res);
            assert_eq!(res.len(), 7);             
        }, |err| {                               
            unreachable!("{:?}", err);
        });

    let mut v = vec![];
    for i in 0..20 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(5, true, v, ControlFlow::ParallelLimit(3))
        .finally_sync(|res| {           
        	println!("Res: {:?}", res);    
            assert!(res.len()>=5 && res.len()<=7);                
        }, |err| {
            unreachable!("{:?}", err);
        });

	let mut v = vec![];
    for i in 0..20 {
        v.push(Deferred::<u32, &str>::new(move ||{ Ok(i) }));
    }
    let _ = Deferred::first_to_promise(2, true, v, ControlFlow::Series)
        .then(|res| {
            println!("Res: {:?}", res);
            assert_eq!(res.len(), 2);                
            Ok(res)
        }, |error| {
        	println!("Error: {:?}", error);
            Err("Error")
        }).sync();

	for _ in 0..5 {
		let promiseA = Promise::new(|| {
			println!("--> Starting Promise A");
			let v = vec![1;1024*1024];			
			println!("<-- End Promise A");
			Ok(v)
		});
		let promiseB = Promise::new(|| {
			println!("--> Starting Promise B");
			let v = vec![2;1024*512];
			println!("<-- End Promise B");
			Ok(v)
		});	
		Promise::<Vec<u8>,&str>::all(vec![promiseA, promiseB]).sync();
		println!("End loop");
	}

	let exter_value = 2;

	let promise = Promise::<_,String>::new(|| {
		thread::sleep_ms(2000);	// Simulate async work
		println!("Inside new");
		if true {
			Err("An error has occurred".to_string())
		} else {
			Ok(12)
		}
	}).success(move |res| {
		println!("Inside success: {}", res);
		Ok(res * exter_value)
	}).fail(|error| {
		println!("Inside fail: {}", error);
		Promise::new(|| {
			thread::sleep_ms(500); // Simulate async work
			println!("Recovery from fail!!!");
			promise_mock(6, 2).success(|res| {			
				println!("Result mock division: {}", res);
				thread::sleep_ms(500); // Simulate async work
				Ok(res as u32)
			}).sync()
		}).sync()
	});


	println!("Hello, in main program!");

	promise.finally_sync(|r| {
		println!("Finally ok: {} ",r);
	}, |e| {
		println!("Finally KO: {}", e);
	})
}