// ======================
// Pre-RFC: inplace types
// ======================
//
// Summary 
// =======
//
// A new 'inplace' keyword denotes a value that will be initialized in place at its final location,
// rather than constructed on the stack and copied into place.
//
// It can be used as both an expression modifier and as a type modifier:

structure.field = inplace expr;
vec.push(inplace expr);

fn create() -> inplace T;
fn create_or_fail() -> Result<inplace T, Error>;

// An 'inplace T' represents a potential value. It can be thought of like a closure that returns
// a 'T', but with a guarantee that the 'T' value will be created in place where it is needed.
//
// It has an internal, compiler-generated initializer which is not externally accessible. It is
// invoked automatically where required.
//
// We also define the type '?inplace T' as a way to accept either a 'T' or 'inplace T', and update
// existing APIs in the standard library to use this type, e.g:

impl<T> Vec<T> {
    pub fn push(&mut self, value: ?inplace T);
}

// By altering function signatures to accept this dual type, support for inplace initialization can
// be added to existing APIs, whilst maintaining backwards compatibility with existing code and
// avoiding API churn.
//
// An 'inplace T' or '?inplace T' can be queried for its Layout, using new functions added to
// alloc::Layout:

impl Layout {
    pub fn for_inplace_value<T>(t: &inplace T) -> Layout where T: ?Sized;
    pub fn for_maybe_inplace_value<T>(t: &?inplace T) -> Layout where T: ?Sized;
}

// By using these functions to first obtain the necessary layouts, container implementations can
// allocate space for a potential value and then initialize it in-place, including for dynamically
// sized types.
//
// Conversions
// ===========
//
// There are two rules for conversion between 'inplace T' and 'T':
//
// 1. A value of type 'inplace T' can be converted to 'T'. Upon conversion, the initializer is
//    executed and the 'inplace T' is consumed, constructing the 'T' value in the destination.
//
// 2. An expression of type 'T' can be converted to 'inplace T' at compile time, if the compiler
//    can see how to safely construct the result of that expression inplace where later used.
//
// Thus, writing a function that returns 'inplace T' does not require any special syntax or style,
// only a suitable expression of type 'T':

fn create_foo() -> inplace Foo {
    Foo { a: 123, b: "hello" }
}

// In this example, the compiler converts from 'Foo' to 'inplace Foo' in order to satisfy the
// return type of the function. Rather than emitting code to allocate and initialize the return
// value on the stack, the compiler generates an initializer which will populate the value at a
// later time when the destination is known. The 'create_foo' function returns what is essentially
// a handle for the potential value.
//
// An 'inplace T' must contain everything needed to infallibly construct the 'T' value. In the
// example above, the effect is similar to if 'create_foo' returned a closure to be evaluated
// later:

fn create_foo() -> FnOnce() -> Foo {
    || Foo { a: 123, b: "hello" }
}

// However, an 'inplace T' is not simply sugar for a closure that returns a value: it carries with
// it the guarantee that the value will be constructed in place where it is required.
//
// This is the main difference between this proposal and RFC 2884, which sought to guarantee
// inplace initialization of return values under certain conditions. A key shortcoming of that
// approach was that its effect was implicit: it required understanding complex rules to know when
// inplace construction could be guaranteed, and it was hard to know when the compiler should warn
// about it not happening.
//
// In this proposal, new syntax allows the usage of in-place initialization to be specified
// explicitly, such that the intent is always clear. Where the compiler cannot see how to achieve
// what is specified, errors can be reported reliably and more clearly.
//
// Scope of deferral
// =================
//
// When a compile-time expression is converted from 'T' to 'inplace T', code to construct the 'T'
// value is deferred to the initializer routine. In the first example above, this conversion was
// caused by the need to convert a 'Foo' to 'inplace Foo' in order to satisfy the return type of
// 'create_foo'.
//
// By explicitly using 'inplace' with a block, the scope of deferral can be expanded, moving
// more code to the initializer:

fn create_thing() -> inplace Thing {
    code_executed_now();
    inplace {
        code_deferred_to_initializer();
        Thing {...}
    }
}

// The scope of deferral is therefore controllable, explicit, and minimised by default. This
// proposal makes no change to the semantics of existing code.
//
// Error handling
// ==============
//
// Code that produces an 'inplace T' must complete any error handling which could prevent it from
// returning a valid 'T' value, prior to the conversion to 'inplace T'. The code deferred to the
// initializer must always result in a valid 'T' value:

fn try_create_thing() -> Result<inplace Thing, Error> {
    fallible_code_executed_now()?;
    inplace {
        infallible_code_deferred_to_initializer();
        Thing {...}
    }
}

// This constraint does not completely preclude further error handling within the initializer, but
// it does limit that code to actions that still produce a valid 'T' value.
//
// Composition
// ===========
//
// Consider the function below:

fn create_bar() -> Result<inplace Bar, Error> {
    Ok(Bar {
        baz: create_baz()?,
        quux: create_quux()?,
    })
}

// In this example, the calls to create_baz() and create_quux() are executed before 'create_bar'
// function returns, along with the construction of the Result. Only the final construction of the
// 'struct Bar', with the captured return values of the two helper functions, is deferred.
//
// Obviously, there is little gain to constructing a structure in place if all of its fields must
// first be created separately, then captured in a pseudo-closure to be moved into place later.
//
// We can ensure that the above example is fully constructed in place by having the helpers it uses
// themselves return 'inplace' results:

fn create_baz() -> Result<inplace Baz, Error>;
fn create_quux() -> Result<inplace Quux, Error>;

// We apply the rule that when an 'inplace T' value is used as a field in another 'inplace'
// structure, its initialization is deferred to the initialization of the outer structure. In other
// words, inplace expressions can be nested and their initializers will be executed together. Thus,
// complex structures can be constructed in place by composition of their parts.
//
// Optionally inplace types
// ========================
//
// The '?inplace T' dual type allows for interoperability and backwards compatibility for code that
// needs to support both inplace and non-inplace values.
//
// In C++, the introduction of 'placement new' required e.g. adding a new 'emplace_back' method to
// std::vector alongside the existing 'push_back' method, causing API churn for code that wanted
// to take advantage of in-place construction.
//
// With this proposal there is no need for Rust to do the same. For instance, Vec::push can be
// rewritten to have the signature:

pub fn push(&mut self, value: ?inplace T);

// The new version can remain compatible with all existing callers, as it can be monomorphized into
// two implementations; one equivalent to the current function that accepts 'T', and one that
// accepts 'inplace T' via a new ABI.
//
// Example
// =======
//
// In a hypothetical simple container supporting DSTs and using fallible allocation, insertion
// might look like:

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

// When the function is monomorphized for 'T', the Layout call returns the layout of the already
// initialized value on the stack. The function allocates a location on the heap, and the pointer
// assignment results in the value being copied from the stacked argument value to the heap.
//
// When monomorphized for 'inplace T', the Layout call retrieves the expected layout for the
// potential value. The function allocates a location on the heap of the correct size. The pointer
// assignment requires a conversion from 'inplace T' to 'T', so the initializer is called to
// construct the value in-place in the newly allocated location. The 'inplace T' is consumed by the
// conversion and dropped.
//
// In the case where an 'inplace T' is passed but allocation fails, the initialization is skipped
// and the 'inplace T' value is dropped unused.
//
// Consider the use of this function on a small embedded system:

arr.append([1u32; 1_000]);             // Wasteful: created on stack, and copied to new allocation.

arr.append(inplace [1u32; 1_000]);     // Created in-place: less stack used, copy avoided.

arr.append([1u32; 1_000_000]);         // Might run out of stack before even trying to allocate!

arr.append(inplace [1u32; 1_000_000]); // Won't deplete stack. Allocation may fail, in which
                                       // case false is returned and initialization is avoided.

// Fallible initialization
// =======================
//
// Allowing for fallible *allocation* is straightforward in the above example, because it is a
// generic problem that applies equally to any type that might be inserted into the container.
//
// Allowing for fallible initialization is more awkward, because it would require every method
// on a container, or other function that accepted an 'inplace T', to be genericised such that
// it could also return a result indicating failure of the initialization.
//
// It may be straightforward to adapt this proposal such that rather than just 'inplace T' we
// have for instance a magic trait 'Inplace<T, E>' in which E is the error type that may be
// optionally returned by the initializer.
//
// However, this would probably require that there be some function call to perform the
// initialization, which would need to be given a pointer to uninitialized memory and return
// an Option<E>. As it stands, many uses of the proposed 'inplace' feature can be made entirely
// without any unsafe code, when used to assign to existing locations such as struct members.
//
// The trait in question might look like:

trait Inplace<T, E> {
    fn layout(&self) -> Layout;
    fn initialize(self, *mut dest: T) -> Option<E>;
}

// The problem with this approach is that it reintroduces the problem of API churn. Rather than
// simply adapting Vec::push() as outlined above, we would need a new call along the lines of:

enum InplaceError<E> {
    AllocFailure(),
    InitFailure(E),
}

impl<T> Vec<T> {
    pub fn try_push<E>(&mut self, value: ?Inplace<T, E>) -> Result<(), InplaceError>;
}

// That said, this might still be nicer than the closure-based approach proposed in #2884.
//
// Self-referential structures
// ===========================
//
// With the ability to require that structures be created in place, it becomes possible in
// combination with the !Unpin trait for self-referential structures to be created safely. This
// would require changes to the borrow checker as well as some syntactic means of self-reference.
//
// Here, we hypothesise reusing the 'inplace' keyword to refer to the eventual destination of an
// inplace expression, but other syntax choices could be used:

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

// This would address the issues affecting the use of Rust in the Linux kernel, as discussed at:
//
// https://lwn.net/Articles/907876/
