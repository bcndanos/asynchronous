extern crate asynchronous;

use asynchronous::Promise;
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
	}).then(move |res| {
		println!("Inside then: {}", res);
		Ok(res * exter_value)
	}).fail(|error| {
		println!("Inside fail: {}", error);
		Promise::new(|| {
			thread::sleep_ms(500); // Simulate async work
			println!("Recovery from fail!!!");
			promise_mock(6, 2).then(|res| {			
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