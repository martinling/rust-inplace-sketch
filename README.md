# Pre-RFC: inplace types

## Summary 

A new `inplace` keyword denotes a value or type that will be initialized in
place at a location.

It can be used as both an expression modifier and as a type modifier:

```rust
structure.field = inplace expr;
vec.push(inplace expr);

fn create() -> inplace T;
fn create_or_fail() -> Result<inplace T, Error>;
```

An `inplace T` represents a potential value. It can be thought of like a
closure that returns a `T`, but with a guarantee that the `T` value will be
created in place where it is needed.

Expressions and blocks of type `T` can be converted to `inplace T` at compile
time from safe code, resulting in an internal initializer created by the
compiler. An `inplace T` can be converted to `T`, invoking the initializer with
the destination.

We also define the type `?inplace T` as a way to accept either a `T` or
`inplace T`, and update existing APIs in the standard library to use this type,
e.g:

```rust
impl<T> Vec<T> {
    pub fn push(&mut self, value: ?inplace T);
}
```

By altering function signatures to accept this dual type, support for in-place
initialization can be added to existing APIs, whilst maintaining backwards
compatibility with existing code and avoiding API churn.

An `inplace T` or `?inplace T` can also be queried for its `Layout`, using new
functions added to `alloc::Layout`:

```rust
impl Layout {
    pub fn for_inplace_value<T>(t: &inplace T) -> Layout where T: ?Sized;
    pub fn for_maybe_inplace_value<T>(t: &?inplace T) -> Layout where T: ?Sized;
}
```

By using these functions to first obtain the necessary layouts, container
implementations can allocate space for a potential value and then initialize it
in-place, including for dynamically sized types.

## Conversions

There are two rules for conversion between `inplace T` and `T`:

1. A value of type `inplace T` can be converted to `T`. Upon conversion, the
   initializer is executed and the `inplace T` is consumed, constructing the
   `T` value in the destination.

2. An expression of type `T` can be converted to `inplace T` at compile time,
   if the compiler can see how to safely construct the result of that
   expression in-place where later used.

Thus, writing a function that returns `inplace T` does not require any special
syntax or style, only a suitable expression of type `T`:

```rust
fn create_foo() -> inplace Foo {
    Foo { a: 123, b: "hello" }
}
```

In this example, the compiler converts from `Foo` to `inplace Foo` in order to
satisfy the return type of the function. Rather than emitting code to allocate
and initialize the return value on the stack, the compiler generates an
initializer which will populate the value at a later time when the destination
is known. The `create_foo` function returns what is essentially a handle for
the potential value.

An `inplace T` must capture everything needed to construct the `T` value. In
the example above, the effect is similar to if `create_foo` returned a closure
to be evaluated later:

```rust
fn create_foo() -> FnOnce() -> Foo {
    || Foo { a: 123, b: "hello" }
}
```

However, an `inplace T` is not simply sugar for a closure that returns a value:
it carries with it the guarantee that the value will be constructed in place
where it is required.

This is the main difference between this proposal and the [placement by return
RFC](https://github.com/rust-lang/rfcs/pull/2884), which sought to guarantee
inplace initialization of return values under certain conditions. A key
shortcoming of that approach was that its effect was implicit: it required
understanding complex rules to know when inplace construction could be
guaranteed, and it was hard to know when the compiler should warn about it not
happening.

In this proposal, new syntax allows the usage of in-place initialization to be
specified explicitly, such that the intent is always clear. Where the compiler
cannot see how to achieve what is specified, errors can be reported reliably
and clearly.

## Scope of deferral

When a compile-time expression is converted from `T` to `inplace T`, code to
construct the `T` value is deferred to the initializer routine. In the simple
example above, this conversion was caused by the need to convert a `Foo` to
`inplace Foo` in order to satisfy the return type, so only the final values of
each field were captured and stored with the initializer, and only the actual
construction of the complete `struct Foo` was deferred.

By explicitly using `inplace` with a block, the scope of deferral can be
expanded, moving more code to the initializer:

```rust
fn create_thing() -> inplace Thing {
    code_executed_now();
    inplace {
        code_deferred_to_initializer();
        Thing {...}
    }
}
```

The scope of deferral is therefore controllable, explicit, and minimised by
default.

## Error handling

Code that produces an `inplace T` must complete any error handling which could
prevent it from returning a valid `T` value, prior to the creation of an
`inplace T`. The code deferred to the initializer must always result in a valid
`T` value:

```rust
fn try_create_thing() -> Result<inplace Thing, Error> {
    fallible_code_executed_now()?;
    inplace {
        infallible_code_deferred_to_initializer();
        Thing {...}
    }
}
```

This constraint does not completely disallow further error handling within the
initializer, but it does limit that code to outcomes that still produce a valid
`T` value.

Code in the initializer must never unwind, since to do so could leave
references pointing to an uninitialized value. The compiler must reject the
code if it cannot compile the initializer without unwinding.

This implies that the compiler must reject any may code in an `inplace` block
that may panic, in any configuration where a panic results in an unwind.
However, it would be acceptable to allow panics in the initializer in a
configuration where a panic results in an abort.

## Composition

Obviously, there is little gain to constructing a structure in place if all of
its fields must first be created separately, then captured in a pseudo-closure
to be moved into place later.

We apply a rule that when an `inplace T` value is used as a field in another
`inplace` structure, its initialization is deferred with the initialization of
the outer structure. In other words, inplace expressions can be nested and
their initializers will be executed together. Thus, complex structures can be
constructed in place by composition of their parts.

Consider the example below:

```rust

struct Bar {
    baz: Baz,
    quux: Quux,
}

fn create_baz() -> Result<inplace Baz, Error>;
fn create_quux() -> Result<inplace Quux, Error>;

fn create_bar() -> Result<inplace Bar, Error> {
    Ok(Bar {
        baz: create_baz()?,
        quux: create_quux()?,
    })
}
```

The `create_bar` function runs all the error checks, and returns either an
`Error`, or an `inplace Bar` which can be used to later initialize the `struct
Bar` in place, composing the effects of its component initializers.

## Optionally inplace types

The `?inplace T` dual type allows for interoperability and backwards
compatibility for code that needs to support both inplace and non-inplace
values.

In C++, the introduction of _placement new_ required e.g. adding a new
`emplace_back` method to `std::vector` alongside the existing `push_back`
method, causing API churn for code that wanted to take advantage of in-place
construction.

With this proposal there is no need for Rust to do the same. For instance,
`Vec::push` can be rewritten to have the signature:

```rust
pub fn push(&mut self, value: ?inplace T);
```

The new version remains compatible with all existing callers. It can be
monomorphized into two implementations: one equivalent to the current function
that accepts `T`, and one that accepts `inplace T` via a new ABI.

## Example

In a hypothetical simple container supporting DSTs and using fallible
allocation, insertion might look like:

```rust
struct DstArray<T> {
    pointers: Vec<*mut u8>,
    _phantom: PhantomData<T>,
}

impl<T> DstArray<T> where T: ?Sized {
    pub fn append(&mut self, value: ?inplace T) -> bool {
        let layout = Layout::from_maybe_inplace_value(&value);
        unsafe {
            let pointer = System.alloc(layout);
            if pointer.is_null() {
                return false;
            }
            *pointer = value;
        }
        self.pointers.push(pointer);
        true
    }
}
```

When the function is monomorphized for `T`, the `Layout` call fetches the
layout of the already initialized value on the stack. The function allocates a
new location on the heap, and the pointer assignment results in the value being
copied from the stacked argument value to the heap.

When monomorphized for `inplace T`, the `Layout` call retrieves the expected
layout for the potential value. The function allocates a location on the heap
of the correct size. The pointer assignment requires a conversion from `inplace
T` to `T`, so the initializer is called to construct the value in-place in the
newly allocated location. The `inplace T` is consumed by the conversion and
dropped.

In the case where an `inplace T` is passed but allocation fails, the
initialization is skipped and the `inplace T` value is dropped unused.

Consider the uses of this function on a small embedded system:

```rust
arr.append([1; 1_000]);             // Wasteful: created on stack, and copied to new allocation.

arr.append(inplace [1; 1_000]);     // Created in-place: less stack used, copy avoided.

arr.append([1; 1_000_000]);         // Might run out of stack before even trying to allocate!

arr.append(inplace [1; 1_000_000]); // Won`t deplete stack. Allocation may fail, in which
                                    // case false is returned and initialization is avoided.
```

## No fallible initialization

This proposal provides no support for fallible initialization. However, it does
support:

- Fallible allocation.

- Error handling which produces a Result prior to the final initialization.

- Composition of error handling by the usual mechanisms, resulting in a
  compound initializer.

- Error handling within the deferred initializer which still produces a valid
  value.

We argue that these benefits are sufficient to justify the proposal without
support for fallible initialization.

In principle, it might be straightforward to adapt this proposal such that
rather than just `inplace T` we have for instance a magic trait `Inplace<T, E>`
in which `E` is an error type that may be potentially returned by the
initializer.

However, since there is no way for an assignment to result in an error, the
conversion to `T` would require some other operation to be invoked to perform
the initialization and optionally return an error. Assuming this were a
function call, it would need to be passed a pointer to uninitialized memory, so
would only be usable in an unsafe context. That would be a significant step
backwards because as it stands, this proposal allows in place initialization
with no unsafe code, except in places where uninitialized memory is already in
use, such as within the implementation of a container type.

Supporting fallible initialization would further require every method on a
container, or other function that accepted an `inplace T`, to be genericised
such that it could also return a result indicating failure of the
initialization. This would reintroduce the API churn problem.

## Self-referential structures

With the ability to require that structures be created in place, it becomes
possible in combination with pinning for self-referential structures to be
created safely. Doing so would require some new syntactic means of
self-reference, as well as changes to the borrow checker if self-references
rather than just self-pointers are to be used.

Here, we hypothesise reusing the `inplace` keyword to refer to the eventual
destination of an inplace expression, but other syntax choices could be used:

```rust
struct ListHead {
    prev: &ListHead,
    next: &ListHead,
    _pin: PhantomPinned,
}

impl ListHead {
    fn new() -> inplace ListHead {
        ListHead {
            prev: &inplace,
            next: &inplace,
            _pin: PhantomPinned,
        }
    }
}
```

This would address issues affecting the use of Rust in the Linux kernel, as
discussed in [this LWN article](https://lwn.net/Articles/907876/).
