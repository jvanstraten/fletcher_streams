# NOTE: WORK IN PROGRESS. NOT A REAL SPEC YET.

Fletcher stream specification
=============================

Introduction
------------

The Apache Arrow project standardizes a way to represent tabular data in linear
memory, to allow high performance access to the table from within a multitude
of programming languages and VMs without serialization. It also specifies a
file format for serializing subsets of these tables (record batches) to files.

Within the software and GPU world this is good enough, since the primitive
instructions on these devices operate on memory. FPGA accelerators, on the
other hand, typically operate on streams. In the same way that it makes sense
to standardize an in-memory format between programming languages to leverage
the strengths and libraries in each, it makes sense to standardize a streaming
format in the FPGA domain, such that supporting computation kernels and
libraries from different vendors can work together out of the box. Furthermore,
an open hardware IP library containing components to connect this streaming
format to the Arrow in-memory format is desirable.

Over the past few years, AXI4 streams, part of the AMBA 4 specification by ARM,
have become the de-facto standard for generic streaming interfaces. An AXI4
stream consists of a `valid`/`ready` handshake, one or more byte lanes that can
be individually masked, a `last` flag for one-dimensional packet boundaries,
and routing information.

A naive approach to specifying an Arrow streaming format would be to simply
require the use of such AXI4 streams. The first issue is then however what
should be streamed. Serialized record batches? The individual column
buffers in sequence, or perhaps interleaved? Clearly, the AXI4 specification
on its own is not enough.

Streaming record batches as AXI4 stream packets would be better in that the
format is suitable for AXI4 streams and that it is unambiguous; however,
kernels are likely to A) only use part of the columns in the table for their
computation and B) operate in a row-oriented fashion. Some kernels,
specifically those operating on graphs, may also need random access. A kernel
might stream the record batch into a local memory, but this simply shifts the
problem, and requires large (off-chip) memories.

A more suitable format would be to stream each individual in-memory Arrow
buffer over a different stream. This allows only the relevant data to be
streamed, allows row-wise access, and, with a suitable command stream from
the kernel to the memory-to-stream (DMA) engine, random access is possible.
Further problems arise, however. First of all, the kernel developer needs to
have an in-depth understanding of the Arrow in-memory format to control the
DMA engine and interpret the streams; kernel developers may instead be
inclined to define their own in-memory format and shift data preparation to
the software domain. Secondly, the byte-oriented format specified by AXI4
streams is not a good fit for streaming the wide variety of data structures
supported by Arrow. For instance, since AXI4 is byte-oriented, streaming
booleans is impractical; one would either have to use only one of the eight
bits of each byte lane (and need a custom DMA engine to do so), or forego
the ability to define validity on per-boolean basis. On the other end of
the spectrum, streaming 32-bit integers is inconvenient since, without
imposing additional restrictions on the stream, the hardware would need to
be capable of reconstructing the integers based on the byte strobe signals.
AXI4 streams are also incapable of representing nested data structures; only
one-dimensional packet boundaries can be given.

Summarizing the above:

 - A standardized streaming format for Arrow data is needed to expand the
   Arrow project into the FPGA accelerator domain. A hardware IP library is
   furthermore needed to convert between this streaming format and the Arrow
   in-memory format.
 - While AXI4 streams are the de-facto standard for streaming interfaces,
   its specification is on its own insufficient to accomplish this goal.
 - Using streams fully conforming to the AXI4 specification is impractical
   for many of the complex data structures supported by Arrow.

This document aims to standardize such a streaming format. A preliminary
version of the hardware IP library to convert between the suggested format and
the Arrow in-memory format is available from the Fletcher project, maintained
by TU Delft.

Logical vs. physical streams
----------------------------

Most of the more complex data types supported by Arrow do not have a fixed
memory footprint per data element. The most common example of this is an array
of strings. Arrow represents these data types as two separate columnar blocks
of memory. In Arrow, these blocks are called buffers, while the logical dataset
they together represent are called arrays. The streaming format requires a
similar abstraction: the streaming representation of an array is called a
logical stream, which is made up of one or more physical streams. Note,
however, that a physical stream is not necessarily equivalent to a buffer; the
streaming format attempts to minimize the number of independent streams as this
complicates the control logic.

For logical streams, we also make the distinction between (normal) linear
streams and random-access streams. The latter includes a return channel that
allows the sink to skip over elements or request elements again, while the
former is a simpler and more conventional unidirectional data channel.
Random-access streams bear some superficial resemblance to read-only
memory-mapped AXI4, but they are fundamentally different: they represent a
stream of an *array* of elements, where the sink can choose which elements it
wants to access within the current array by means of their index. The primary
use case for a random-access stream is to let a kernel access an Arrow array as
it sees fit, while the host controls which arrays to access in sequence as part
of independent kernel invocations.

Physical streams
----------------

Physical Fletcher streams consist of a selection of the following signals, not
including clock and reset. All signals are synchronous to a common clock
domain.

| Name    | Origin | Width | Default   | Purpose                                                       |
|---------|--------|-------|-----------|---------------------------------------------------------------|
| `valid` | Source | 1     | `'1'`     | Stalling the data stream due to the source not being ready.   |
| `ready` | Sink   | 1     | `'1'`     | Stalling the data stream due to the sink not being ready.     |
| `data`  | Source | N x M | n/a       | Data transfer of N M-bit elements.                            |
| `strb`  | Source | N     | all `'1'` | Encodes which lanes are valid.                                |
| `stai`  | Source | C     | 0         | Encodes the index of the first valid element in data.         |
| `endi`  | Source | C     | N-1       | Encodes the index of the last valid element in data.          |
| `empty` | Source | 1     | `'0'`     | Encoding zero-length packets.                                 |
| `last`  | Source | D     | all `'1'` | Indicating the last transfer for D levels of nested packets.  |
| `ctrl`  | Source | U     | n/a       | Additional control information carried along with the stream. |

These signals are described in detail in the following sections.

In the table, the following parameters are used:

 - N is the maximum number of elements that can be transferred in a single
   cycle.
 - M is the bit-width of each element.
 - D is the dimensionality of the datatype represented by the stream, or
   alternatively, the packet nesting level.
 - C is the width of the `stai` and `endi` signals, which must always be equal
   to ceil(log2(N)).
 - U is the width of the `ctrl` signal, of which the significance is not
   defined by (this layer of) the specification.

These parameters are defined by the requirements of the source and sink that
the stream is connected to. The values for N, M, D, (and indirectly, C,) must
be equal for the source and sink. U can be different, in which case the stream
uses the maximum width of the values required by the source and sink, and the
signal is resized on either side as if it were an unsigned number.

In addition to these parameters, there is an additional "hidden" parameter
called the normalization level (L). The higher this level, the more restricted
the transfers on the stream are: the source has to conform to additional rules,
while the sink can assume that these rules are adhered to. The levels are:

 - Level 0: no additional rules imposed.
 - Level 1: the `strobe` bits are always one.
 - Level 2: `stai` is always zero. That is, if a transfer contains data, the
   first element is always in the first lane.
 - Level 3: the rules of level 2, and `endi` is always N-1 if `last` is 0. That
   is, the element with index i is always in lane i mod N.
 - Level 4: the rules of level 3, and `empty` is always `'0'` if `last` is 0.
   That is, the `last` marker cannot be postponed to an independent transfer,
   even if the total number of elements transfered is divisible by N.
 - Level 5: the rules of level 4, but `empty` is always `'0'` regardless of
   `last`. That is, empty sequences are not supported.
 - Level 6: the rules of level 5, and `valid` is must remain `'1'` until `last`
   a transfer with nonzero `last` is handshaked. That is, the stream is never
   interrupted (unless backpressured through `ready`) during a packet.

The normalization level provided by a source must be greater than or equal to
the level expected by a sink, or behavior is undefined.

It is possible to construct generic components that vary N and increase L from
any level to level 3, allowing an otherwise incompatible source/sink pair to be
connected.

### `valid` and `ready`

The `valid` and `ready` signals fulfill the same function as the AXI4-steam
`TVALID` and `TREADY` signals.

 - The source asserts `valid` high synchronous to the rising edge of the clock
   signal common to source and sink in the same cycle in which it presents
   valid data on the remaining signals.
 - Source-generated signals other than `valid` are don't-care while `valid` is
   low.
 - The sink asserts `ready` high when it is ready to consume the current stream
   transfer.
 - The source must keep all source-generated signals (including valid) stable
   after asserting `valid`, until the first rising edge of the clock during
   which `ready` is asserted.
 - A transfer is considered handshaked when both `valid` and `ready` are
   asserted high at the rising edge of the common clock signal.
 - `ready` is don't-care when `valid` is low. Sources must therefore not wait
   for `ready` to be asserted before asserting `valid`. Conversely, sinks may
   wait for `valid` to be asserted before (possibly combinatorially) asserting
   `ready`.
 - It is recommended for `valid` to be low while the source is being reset, and
   for `ready` to be low while the sink is being reset. This allows source and
   sink to have independent reset sources without loss of data.

Example timing:

```
           __    __    __    __    __    __    __
 clock |__/  \__/  \__/  \__/  \__/  \__/  \__/  \_
       |          ___________       ___________
 valid |_________/          :\_____/    :     :\____
       |                ____________________________
 ready |_______________/    :           :     :
       |          ___________       _____ _____
others |=========<___________>=====<_____X_____>====
                            :           :     :
                            ^           ^     ^
                        stream transfers occur here
```

### `data`

The `data` signal carries all the data transferred by the stream. Two formats
are specified:

 - A bit vector N x M in width (`N*M-1 downto 0`), where N is the maximum
   number of elements that can be transferred in a single cycle, and M is the
   bit-width of each element. Elements are ordered LSB-first.

 - An array of N records (`0 to N-1`), each record/struct containing entries
   descriptive for the transferred element, given that the record can be
   serialized to M bits. The serialization of these records is described in the
   data type serialization section.

Note that these two formats are merely syntactic sugar for the same bundle of
wires, so conversion between the two does not result in additional hardware.
It is recommended to use the bit-vector format on the periphery of IP cores.
The array-of-records format can be used internally to improve code readability,
given appropriate tooling support.

For either format, the following specifications apply.

 - The `data` signal is don't-care while `valid` is not asserted or `empty` is
   asserted.
 - While `valid` is asserted and `empty` is not asserted, only elements with
   index `stai` to `endi` (inclusive) carry significance. The remainder of the
   elements are don't-care.

### `empty`

The `empty` signal is used to encode empty packets, and to delay transfer of
packet boundary information when such information is not known during the last
transfer containing actual data.

 - The `empty` signal is don't-care while `valid` is not asserted.
 - When `empty` is asserted, only control information is transferred. The
   `data`, `stai`, and `endi` signals are therefore don't-care.

### `stai` and `endi`

For streams that can carry more than one element per cycle (N > 1), the
`stai` (start index) and `endi` (end index) signals encode how many and which
of the element lanes are significant. They are C-bit vectors (`C-1 downto 0`),
where C is equal to ceil(log2(N)).

 - The `stai` and `endi` signal is don't-care while `valid` is not asserted or
   while `empty` is asserted.
 - `stai` must always be less than or equal to `endi`.

### `last`

The `last` signal marks a transfer as being the last for a certain (nested)
packet level. It is an D-bit vector (`D-1 downto 0`). Intuitively, the
structure serialized by a Fletcher stream can be seen as D levels of nested
lists.

 - The `last` signal is don't-care while `valid` is not asserted.
 - The LSB is used to terminate the innermost subpackets. The MSB is used to
   terminate the outermost packet.
 - It is illegal to terminate a packet without also terminating all contained
   subpackets (intuitively, violating this would encode an inner list that
   somehow extends beyond the list it is an element of, which is nonsensical).
   Therefore, in transfers where `empty` is not asserted, the `last` vector
   must be a thermometer code. For example, for `D=3`, only the following
   values are valid: `"000"`, `"001"`, `"011"`, and `"111"`.
 - The `empty` flag can be used to delay packet termination. In this case, the
   `last` value need not always be thermometer code. For instance, `"001"`
   without empty followed by `"110"` with empty is a legal way to terminate the
   outermost packet. However, each packet nesting level must only be terminated
   once, and they must be terminated in inner to outer order. For instance,
   `"010"` empty followed by `"101"` with empty is illegal because the order is
   violated. `"001"` without empty followed by `"111"` with empty is legal, but
   encodes an empty innermost packet before the outermost packet is closed.

### `ctrl`

The `ctrl` signal can be used to carry additional non-element data along with
the stream. It therefore acts somewhat like data, but is not affected by
`stai`, `endi`, or `empty`. It is a U-bit vector (`U-1 downto 0`). The
significance of the signal is not specified by this layer of the specification.

Typically, the `ctrl` signal actually consists of multiple logical signals.
Implementations are free to represent these logical signals however they want,
as long as they are serialized to the canonical `ctrl` bit vector when the
stream is passed through IP cores that are not or need not be aware of their
significance.

 - The `ctrl` signal is don't-care while `valid` is not asserted.

Logical streams
---------------

A single physical Fletcher stream can only transfer elements with a fixed size
known at design time, or uniformly nested lists thereof. In order to transfer
more complex data types, multiple physical streams have to work together, and
additional specification is needed to specify which physical data bit does
what.

Before attempting to specify this, let us first formally describe the set of
supported data types by means of a recursive grammar, defined as follows:

    # Top-level rule
    type = bits | list | vector | struct | union ;

    # Comma-separated list of types
    types = type
          | type , "," , types ;

    # Primitive type: a positive number of bits with undefined representation
    bits = "b" , positive number ;

    # Sequences types, mapping to Arrow arrays and nested lists
    list = "[" , type , "]" ;
    vector = "<" , type , ">" ;

    # Concatenations of a number of elements with different types
    struct = "(" , types , ")" ;

    # Alternations of two or more different types, possibly including null
    union = "{" , type , "," , types , "}"
          | "{" , "NULL" , "," , types , "}" ;

Each of these grammar construction rules is accompanied by a rule for
constructing the set of physical streams needed to represent the logical
stream for that type. To define these, we also need to formaly define what
constitutes a logical stream. We define this as S physical streams with
parameters M_i and D_i for physical stream i ∈ 0..S-1, where S ≥ 1. The first
physical stream (i = 0) is referred to as the primary stream, while the zero
or more other streams are referred to as the secondary streams. Within the
physical streams, the physical data element is subdivided into one or more
fields, ordered LSB-first.

### Bits

The `bits` type used to represent a primitive datum with a fixed number of
bits B (denoted `b<B>` in the grammar, where `<B>` is the positive integer
representing the number of bits). Common examples of this are signed and
unsigned two's complement numbers, floating point numbers, and characters.
The logical stream for the `bits` type has the following parameters:

    S = 1
    M_0 = B
    D_0 = 0

### Lists and vectors

A `list` with element type `T` (denoted `[T]` in the grammar) represents a
sequence of elements of type `T` of which the length is not known at
design-time. It is important to realize at this point that for instance `[b8]`
represents a stream of byte sequences, not just a stream of bytes — a stream
of bytes has the type `b8`. For lists, these sequences are delimited by means
of adding a `last` signal to each of the physical streams that represents `T`,
thus:

    S = S^T
    M_i = S^T_i         | i ∈ 0..S-1
    D_i = D^T_i + 1     | i ∈ 0..S-1

A `vector` with element type `T` (denoted `<T>` in the grammar) also represents
a sequence of elements of type `T` of which the length is not known at
design-time, but the sequence boundaries are communicated by means of a (32-bit)
length stream, flowing independently to the data stream. Thus:

    S = S^T + 1
    M_0 = 32
    D_0 = D^T_0
    M_i+1 = S^T_i       | i ∈ 0..S-1
    D_i+1 = D^T_i       | i ∈ 0..S-1

Sinks of vector streams may assume that the length of the vector is
communicated to them before they have to start accepting the vector data.
This allows the sink to for instance allocate space for the sequence before
it processes it further. This means that components that produce vectors must
ensure that this length is indeed made available before they wait for the sink
to accept any element, or a deadlock can occur.

Vectors are typically more complicated to implement properly, but can be more
performant than lists. Specifically, a stream of type `[T]` can only transfer
one `T` per cycle, regardless of its `N` parameter and the size of the list,
due to the sequence boundary being encoded with the `last` control signal.
Streams of vectors do not have this limitation; they can transfer `N_0`
sequences per cycle, as long as the throughput is not limited by the widths of
the secondary streams.

A stream of vectors can be transformed into a stream of lists very simply, by
counting the number of elements on the incoming secondary stream and indicating
`last` when the element count hits the incoming length. The opposite is much
more difficult however, as determining the sequence length requires consumption
(and thus buffering) of the incoming data stream. Also, vectors are limited to
2^32-1 elements, while list length is unbounded.

### Structs

A `struct` with element types `T`, `U`, ... (denoted `(T,U,...)`) represents a
data type built up out of the concatenation of its element types (similar to a
`struct` in C). The logical stream for such a `struct` is constructed with the
following algorithm:

    # An empty struct (if it would be legal) consists of a single primary
    # stream with a zero-sized data element.
    S = 1
    M_0 = 0
    D_0 = 0

    for each element type T:
        if D^T_0 = 0:
            # Concatenate the data elements of the primary streams together,
            # to reduce the number of physical streams for the struct as much
            # as possible.
            M_0 += M^T_0

        else:
            # When the struct element type is a list, we can't join the data
            # with the other struct elements, so the list element stream
            # becomes a secondary stream.
            M_S = M^T_0
            D_S = D^T_0
            S += 1

        # Append any secondary streams of the struct element to our list of
        # secondary streams.
        for i in 1 to S^T - 1:
            M_S = M^T_i
            D_S = D^T_i
            S += 1

    # If the struct contains only lists, the primary stream will still be
    # empty. Since an empty stream makes no sense, we special-case it away.
    if M_0 = 0:
        S -= 1
        for i in 0 to S - 1:
            M_i = M_i+1
            D_i = D_i+1

For simple structures such as `(b1,b2)`, this means that the logical stream
behaves as `b3` would, with the data elements concatenated LSB-first.

### Unions

A `union` with option types `T`, `U`, ... (denoted `{T,U,...}`) represents a
data type built up out of the alternation of its option types (similar to a
`union` in C). `union`s can also be nullable, in which case the first option
(0) is reserved for null. The option chosen for each transfered union is
encoded by means of a ceil(log(|options|))-bit unsigned number at the LSB end
of the primary data stream element, where |options| is the number of options
including null. The data is encoded similar to a struct, but the elements in
the primary stream are overlapped (LSB-aligned). Any secondary streams of
union options that have not been selected will not transfer any data (not even
an empty sequence). The algorithm for constructing the logical stream of a
union is as follows:

    # The first field in the primary stream is the union option.
    S = 1
    M_0 = ceil(log2(|options|))
    D_0 = 0

    for each element type T:
        if D^T_0 = 0:
            # Overlap the data elements of the primary streams if possible.
            M_0 = max(M_0, M^T_0)

        else:
            # When the union option type is a list, we don't merge the
            # streams.
            M_S = M^T_0
            D_S = D^T_0
            S += 1

        # Append any secondary streams of the union option to our list of
        # secondary streams.
        for i in 1 to S^T - 1:
            M_S = M^T_i
            D_S = D^T_i
            S += 1

### Mapping between Arrow and Fletcher stream types

Arrow types and Fletcher stream types do not map entirely one-to-one; in some
cases there multiple Fletcher stream representations are possible. It is then
up to the user to choose which representation is most suitable for the
application, along with the N parameter for each physical stream.

In general, the following mappings are possible.

| Arrow type                      | Fletcher stream type                                        |
|---------------------------------|-------------------------------------------------------------|
| Non-nullable arrays             | `list` or `vector` of array type                            |
| Nullable arrays                 | `list` or `vector` of `union` of null and the array type    |
| Fixed-length primitive types    | `bits`                                                      |
| Variable-length primitive types | `list` or `vector` of `bits`                                |
| Nested lists                    | `list` or `vector`                                          |
| Nested structs                  | `struct`                                                    |
| Nested unions                   | `union`                                                     |
| Dictionaries                    | encoded as dictionary index (`bits`) or as the mapped value |

Random-access streams
---------------------

When data originates from a (chunked) Arrow array, allowing a kernel to perform
random access costs very little, but greatly increases the potential for
development of efficient solutions. This is













Data type serialization
-----------------------

Arrow supports a number of recursively defined data types for arrays, which the
streams must be able to carry. For simple data types, a single stream is
sufficient, but in general a data type is handled by one or more independent
streams, each with their own `M` (and interpretation of those `M` bits) and
`D` parameter determined by the type. The values for the `N`, `C`, and `U`
parameters are left up to the designer and/or implementation.

We first formally define a type system for such stream bundles that is a
superset of the types supported by Arrow. Then we define how the stream bundles
can be constructed from any such type.

### Type system

We define two primitive types: null, written as `0` for brevity, and bit
vectors, written as `b<N>`, where `<N>` is a positive integer. The `0` type is
only used within unions. We can then recursively combine these types in the
form of structs (denoted `(T,S,...)`), unions (denoted `{T,S,...}`), lists
(denoted `[T]`), and vectors (denoted `<T>`).



Fletcher streams are however defined on a lower
abstraction level than Arrow. Our type system is defined as follows.

 - The primitive type is an element of N bits, named `b<N>` (for example,
   `b8` for a byte).

 - Structures: one or more elements with potentially different types grouped
   together. This is denoted with parentheses, for instance `(b8,b1)` describes
   a structure with a byte and a bit. When serializing, elements are ordered
   LSB-first, traversing any hierarchy depth-first; for instance,
   `(b4,(b1,b2),b8)` is serialized as `8888888822114444`.

 - Unions and nullables: two or more elements with potentially different types
   of which only one is valid. This is denoted with curly braces, for instance
   `{b8,b1}` describes a union of a byte and a bit. Unions optionally allow for
   a special null case as well, indicated with the type `0`, which should be
   the first option; for instance, `{0,b8}` represents a nullable byte. Unions
   are serialized as a structure consisting of an identifier field of size
   ceil(log2(N)), where N is the number of options, followed by a data field of
   the maximum size of the possible types, LSB-aligned. The identifier field
   indexes which of the union elements is valid, starting from 0. For instance,
   for the type `{0,b4,b8}`, the serialization behaves like `(b2,b8)`, which is
   serialized as `8888888822`; the value `----010101` for instance denotes the
   second type (`b4`), with value 5.

Nested Arrow lists and Arrow arrays cannot be represented as simple element
types, as they do not have a fixed width. Such variable-length types often
require multiple parallel streams. These streams each get their own handshake.
To simplify the interfaces in VHDL, the signals for these streams may be
concatenated, turning each signal (including the `valid` and `ready` handshake
signals) into bit vectors. When this is done, the signals are concatenated
LSB-first.

Two distinct ways are defined to represent variable-length types, each having
their own advantages and disadvantages. These are named "lists" and "vectors"
to distinguish them, but it is important to realize that they are used to
represent the same Arrow types! The differences are as follows:

 - The list representation, denoted `[T]` for a sequence of elements of type
   `T`, uses an additional `last` signal to convey where the list boundaries
   are. This allows for potentionally infinitely long sequences, or sequences
   where the length is not known in advance. However, at most a single sequence
   can be transferred per cycle, regardless of the number of elements per cycle
   of the stream (`N`).

 - The vector representation, denoted `<T>` for a sequence of elements of type
   `T`, uses two streams: a length stream of type `b32`, and a data stream of
   type `T`. The dimensionality of both streams (`D`) is the same; the data
   stream is said to transfer the flattened data of the stream of lists. This
   allows for multiple sequences to be transferred per cycle by increasing `N`
   of the length stream, assuming that the sequences are short enough. However,
   the length of each sequence is limited to 4294967295. Furthermore, sources
   of such streams must make the length for a sequence available to the sink
   before sending the first data element, otherwise deadlocks may occur;
   therefore, the length must be known in advance.

Special cases arise for complex structures that combine lists and vectors with
structs and unions. Similar to vectors, these require multiple streams running
alongside each other. The following semantics are defined:

 - For structures containing lists, such as `(T,[S])`, each list in the
   structure, and (if there is at least one element), the non-list elements
   concatenated together, get their own stream. In the non-list element stream
   the lists are ignored during serialization; for instance, for
   `([b3],b4,[[b5]],b6,[b7])`, the non-list stream is serialized as
   `6666664444`. `b3`, `b5`, and `b7` each get their own stream, with `D`
   incremented appropriately. The streams are ordered `(b4,b6)`, `[b3]`,
   `[[b5]]`, `[b7]`; the non-list stream is always first, followed by the list
   streams in struct order.

 - For structures containing vectors, such as `(T,<S>)`, the vector lengths are
   carried along with the non-list stream as if the type was `(T,b32)`, and the
   vector data is carried by an independent stream of type `S`. When combined
   with lists, the independent data streams follow struct order; for instance,
   for `(<b3>,b4,[[b5]],b6,<b7>)`, the order and datatypes of the streams are
   `(b32,b4,b6,b32)`, `b3`, `[[b5]]`, and `b7`.

 - The synthesis of streams for mixtures of lists and vectors follow from the
   above rules. For example, `[<b3>]` results in a stream of type `[b32]` for
   the list of vector lengths, and a stream of type `[b3]` for the vector data.
   Note that only the data within the vector container is flattened, so the
   data stream is still a one-dimensional list. The data `[<1, 2, 3>, <4, 5>]`
   would be transferred as `[3, 2]` on the first stream, and as
   `[1, 2, 3, 4, 5]` for the second stream. The other way around, `<[b3]>`,
   results in a different (and less useful) form, being a length stream of
   type `b32` and a data stream of type `[b3]`. The data `<[1, 2, 3], [4, 5]>`
   would then be transferred as `2` on the first stream, and as
   `[1, 2, 3], [4, 5]` for the second stream.

 - Unions containing lists, such as `{T,[S]}`, are serialized as two streams,
   one for the identifier field and one for the unioned data. All data is
   always passed through the secondary stream, regardless of whether the data
   is of a list type or not. The dimensionality of the data stream is taken
   from the highest dimensionality necessary. For instance, for
   `{d2,[[d3]],[d4]}`, the non-list stream consists of `b2` for the identifier,
   and the data stream is of type `[[d4]]` respective to it, carrying either
   a single `d2`, a two-dimensional `d3`, or a one-dimensional `d4`,
   LSB-aligned and surrounded by as many single-element lists as needed. That
   is to say, `[1, 2, 3]` for union option `[d4]` will be represented as
   `[[1, 2, 3]]`. Null data behaves like a 0-width vector, and will thus be
   represented as `[[0]]`.

 - For unions containing vectors, the vector data type is regarded as its `b32`
   length, and the data stream is always inferred independently.


These streams may be "concatenated" in hardware when necessary; in
   this case, each signal (including `valid` and `ready`) is concatenated to
   a vector, LSB-first in struct order.

Stream primitives
-----------------

A set of primitive operations for generic Fletcher streams is defined in this
section. Any Fletcher stream IP library implementation should include these
components.

### Buffer

A stream buffer consists of a number of slice registers or and/or FIFO that can
store a configurable number of stream transfers. Buffers are used to break
critical paths and wherever a FIFO.

### Xclock

An xclock connects a stream in one clock domain to a stream in another clock
domain.

### Normalizer

A normalizer takes any valid Fletcher stream and converts the transfers to

TODO

A single logical stream of data can be encoded in many different ways by a
Fletcher stream, due to the `stai`, `endi`, and `empty` signals. In this
section we define the notion of a normalized Fletcher stream, for which there
is a one-to-one mapping between a logical data stream and the Fletcher stream
transfers for a given value for N (the number of elements per cycle/element
lanes in the stream).

For a Fletcher stream to be considered normalized, the following additional
requirements must be adhered to while `valid` is asserted:

 - `empty` may only be asserted when `last` is nonzero.
 - `endi` must be N-1 unless `last` is nonzero.
 - `stai` must be 0 at all times and may thus be omitted.
 - The `last` vector must be thermometer-coded at all times. That is, if bit
   *i* is asserted, bit *i*-1 must also be asserted, for all *i* in 1..D-1.

The process of normalizing a Fletcher stream requires some logic. This logic
necessarily has the following characteristics:

 - N'xM N-wide multiplexers are needed to be able to connect all incoming
   element lanes to all outgoing element lanes, where N relates to the incoming
   stream and N' relates to the outgoing stream. For wide streams, an efficient
   implementation of the normalizer, and current FPGA technology, this
   multiplexer is dominates the resource utilization of the normalizer.
 - If *i*xN' elements have been received at the input, no output can be
   generated yet, because subsequent transfers with the `empty` flag set may
   affect the `last` vector for the next output transfer. The sole exception
   is when the MSB of the `last` vector is also set. Thus, even if the input is
   normalized, all transfers but the last transfer of the outermost packet will
   be withheld until the next is available. This is important to consider when
   the source may block until the sink has processed all data sent by it up to
   that point; this may cause a deadlock when a normalizer is in between.

### Resizing (serialization or parallelization)

The process of resizing a Fletcher stream is the process of varying N; that is,
varying the number of element lanes/elements per cycle. This is commonly done
before or after a clock domain crossing.

A distinction is made between complete resizing and incomplete resizing.
Complete resizing is equivalent to normalization; as many element lanes are
used on the output stream as possible in all transfers. Incomplete resizing
does not require this. For instance, simply increasing N without modifying
the stream transfers at all is a valid form of incomplete parallelization.
An incomplete serializer may consider only one incoming transfer at a time,
which may be suboptimal if the ratio between the input element lane count
and the output element lane count is non-integer or the input stream is not
normalized. However, significantly less logic may be needed for an adequate
incomplete resizer versus a complete resizer.

Reshaping

