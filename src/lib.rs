/*! `SharedTakeOnce`: a heap-allocated, shared box that can be consumed.

A `SharedTakeOnce<T>` is a reference-counted pointer
to a `T` value stored on the heap,
with a `take` method that consumes the pointer and gives you ownership of the `T`,
if you are the first to call it.
Subsequent calls to `take` return `None`.
The effect is similar to `Rc<RefCell<Option<T>>>`,
but with less overhead and a nicer API:
A `SharedTakeOnce` is just a heap pointer to a `T` preceded by a reference count.

There are two versions: `sync::SharedTakeOnce` and `non_sync::SharedTakeOnce`.
The former implements `Send` and `Sync` while the latter does not.
The difference is analogous to that between `Arc` and `Rc`:
`sync::SharedTakeOnce` uses slightly more expensive atomic operations,
in exchange for being safely shareable between threads.

Here's an example that creates two handles to a vector,
and then takes the vector via the second handle:

```rust
use shared_take_once::non_sync::SharedTakeOnce;

let handle = SharedTakeOnce::new(vec![1, 2, 3]);
let alias = handle.clone();

// Take ownership of the vector via alias.
let mut v = alias.take().unwrap();
v.push(4);
println!("{:?}", v); // prints "[1, 2, 3, 4]"

// Now the original handle is empty.
assert!(handle.take().is_none());
```

You can use `SharedTakeOnce` to share a `FnOnce` closure between
the 'fulfill' and 'reject' callbacks of a `js_sys::Promise`:

```ignore
use wasm_bindgen::closure::Closure;

let vec = vec![1, 2, 3];
let callback = move || { drop(vec); /* can't be run twice */ };

// Make two `SharedTakeOnce` handles referring to `closure`.
let handle_1 = SharedTakeOnce::new(closure);
let handle_2 = handle_1.clone();

// Each `wasm_bindgen` closure captures one of the handles.
// Only one closure should ever be called, so we can just `unwrap`
// the value returned by `take`.
let fulfill = Closure::once(move |_| handle_1.take().unwrap()());
let reject = Closure::once(move |_| handle_2.take().unwrap()());

let _ = promise.then2(&fulfill, &reject);
```

*/

pub mod non_sync {
    use std::{cell, mem};
    
    struct Inner<T> {
        /// Positive reference count if occupied, negative reference count if taken.
        ref_count: isize,
        value: mem::MaybeUninit<T>,
    }

    pub struct SharedTakeOnce<T>(*mut cell::UnsafeCell<Inner<T>>);

    impl<T> SharedTakeOnce<T> {
        pub fn new(value: T) -> Self {
            let inner = Inner {
                ref_count: 1,
                value: mem::MaybeUninit::new(value),
            };
            SharedTakeOnce(Box::into_raw(Box::new(cell::UnsafeCell::new(inner))))
        }
        pub fn take(self) -> Option<T> {
            // Safety: Since `self` exists, the reference count must not be
            // zero, so the `Inner` is still there. And since we are `!Send` and
            // `!Sync` because of the `UnsafeCell`, this is the only thread that
            // can see this value, so there are no other mutable references to
            // the `Inner`, so we can construct one here.
            let inner: &mut Inner<T>  = unsafe { (*self.0).get_mut() };
            match inner.ref_count {
                n if n > 0 => {
                    // Safety: ref_count is positive, so `value` is occupied.
                    let value = unsafe { inner.value.assume_init_read() };
                    // Negate `ref_count` to mark the `Inner` as empty.
                    inner.ref_count = -inner.ref_count;
                    Some(value)
                }
                n if n < 0 => {
                    None
                }
                _ => unreachable!("SharedTakeOnce should have been freed already"),
            }
            // `self` is dropped here, which adjusts the refcount and frees
            // the `Inner` if needed.
        }
    }

    impl<T> Drop for SharedTakeOnce<T> {
        fn drop(&mut self) {
            // Safety: Since `self` exists, the reference count must not be
            // zero, so the `Inner` is still there. And since we are `!Send` and
            // `!Sync` because of the `UnsafeCell`, this is the only thread that
            // can see this value, so there are no other mutable references to
            // the `Inner`, so we can construct one here.
            let inner: &mut Inner<T>  = unsafe { (*self.0).get_mut() };
            match inner.ref_count {
                n if n > 1 => {
                    inner.ref_count -= 1;
                }
                1 => {
                    // Safety: ref_count is positive, so `value` is occupied.
                    drop(unsafe { inner.value.assume_init_read() });
                    // Safety: ours was the last pointer to the UnsafeCell.
                    drop(unsafe { Box::from_raw(self.0) });
                }
                -1 => {
                    // Safety: ours was the last pointer to the UnsafeCell.
                    drop(unsafe { Box::from_raw(self.0) });
                }
                n if n < -1 => {
                    inner.ref_count += 1;
                }
                n => {
                    assert_eq!(n, 0);
                    unreachable!("ref_count is zero, but SharedTakeOnce exists");
                }
            }
        }
    }

    impl<T> Clone for SharedTakeOnce<T> {
        fn clone(&self) -> Self {
            // Safety: Since `self` exists, the reference count must not be
            // zero, so the `Inner` is still there. And since we are `!Send` and
            // `!Sync` because of the `UnsafeCell`, this is the only thread that
            // can see this value, so there are no other mutable references to
            // the `Inner`, so we can construct one here.
            let inner: &mut Inner<T>  = unsafe { (*self.0).get_mut() };
            assert_ne!(inner.ref_count, 0);
            inner.ref_count += inner.ref_count.signum();
            SharedTakeOnce(self.0)
        }
    }
}

#[test]
fn drop_two() {
    use std::rc::Rc;
    use non_sync::SharedTakeOnce;
    
    let counter = Rc::new(());

    let handle1 = SharedTakeOnce::new(counter.clone());
    assert_eq!(Rc::strong_count(&counter), 2);

    let handle2 = handle1.clone();
    assert_eq!(Rc::strong_count(&counter), 2);

    drop(handle1);
    assert_eq!(Rc::strong_count(&counter), 2);

    drop(handle2);
    assert_eq!(Rc::strong_count(&counter), 1);
}

#[test]
fn take_one_drop_one() {
    use std::rc::Rc;
    use non_sync::SharedTakeOnce;
    
    let counter = Rc::new(());

    let handle1 = SharedTakeOnce::new(counter.clone());
    assert_eq!(Rc::strong_count(&counter), 2);

    let handle2 = handle1.clone();
    assert_eq!(Rc::strong_count(&counter), 2);

    drop(handle1.take());
    assert_eq!(Rc::strong_count(&counter), 1);

    drop(handle2);
    assert_eq!(Rc::strong_count(&counter), 1);
}
