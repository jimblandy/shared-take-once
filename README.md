# shared-take-once

`SharedTakeOnce`: a heap-allocated, shared box that can be consumed.

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

```rust
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

