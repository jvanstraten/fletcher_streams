# NOTE: WORK IN PROGRESS. NOT A REAL SPEC YET.

OpenTide stream specification
=============================

Background and motivation
-------------------------

As FPGAs become faster and larger, they are increasingly used within a
data center context to accelerate computational workloads. FPGAs can already
achieve greater compute density and power efficiency than CPUs and GPUs for
certain types of workloads, particularly those that rely on high streaming
throughput and branch-heavy computation. Examples of this are decompression,
SQL query acceleration, and pattern matching. The major disadvantage of FPGAs
is that they are more difficult to program than CPUs and GPUs, and that
algorithms expressed imperatively in the CPU/GPU domain often need to be
redesigned from the ground up to achieve competitive performance. Both the
advantages and disadvantages are due to the spatial nature of an FPGA: instead
of programming a number of processors, an FPGA designer "programs" millions of
basic computational elements not much more complex than a single logic gate
that all work parallel to each other, and describes how these elements are
interconnected. This extreme programmability comes at a cost of roughly an
order of magnitude in area and an order of magnitude in performance compared
to the custom silicon used to make CPUs and GPUs. Therefore, while imperative
algorithms can indeed be mapped to FPGAs more or less directly through
high-level synthesis (HLS) techniques or the use of softcores, typically an
algorithm needs at least two orders of magnitude of acceleration through clever
use of the spatial nature of the FPGA to be competitive.

Unfortunately, the industry-standard toolchains needed to program FPGAs only
take VHDL, Verilog, SystemC, (more recently through HLS) a subset of C++,
and/or visually designed data flow graphs as their input. The first three
provide few abstractions above the level of a single wire: while they do allow
the use of data types such as integers and structures to represent bundles of
wires, all control of what the voltages on those bundles of wires represent at
a certain point in time (if anything) remains up to the programmer. The latter
two techniques raise the bar slightly by presenting the designer with streams
and memory. However, they are vendor-specific, often require additional
licensing fees over the more traditional design entry methods, and in some
cases are even specific to a certain version of the vendor tool and/or a
certain FPGA device family.

This situation has given rise to a number of open-source projects that take
higher-level languages and transform them to vendor-agnostic VHDL or Verilog.
Examples of this are Chisel/FIRRTL and Clash, using generative Scala code and a
Haskell-like functional programming language as their respective inputs. Both
tools come with their own standard libraries of hardware components that can be
used to compose accelerators out of smaller primitives, similar to the data
flow design method described earlier, but with textual input and the advantage
of being vendor and device agnostic.

With the advent of these data flow composition tools, it is increasingly
important to have a common interface standard to allow the primitive blocks to
connect to each other. The de-facto standard for this has become the AMBA AXI4
interface, designed by ARM for their microcontrollers and processors. Roughly
speaking, AXI4 specifies an interface for device-to-memory connections (AXI4
and AXI4-lite), and a streaming interface (AXI4-stream) for device-to-device
connections.

While AXI4 and AXI4-lite are of primary importance to processors due to their
memory-oriented nature, AXI4-stream is much more important for FPGAs due to
their spatial nature. However, because AXI4-stream is not originally designed
for FPGAs, parts of the specifications are awkward for this purpose. For
instance, AXI4-stream is byte oriented: it requires its data signal to be
divided into one or more byte lanes, and specifies (optional) control signals
that indicate the significance of each lane. Since FPGA designs are not at all
restricted to operating on byte elements, this part of the specification is
often ignored, and as such, any stream with a `valid`, `ready`, and one or more
`data` signals of any width has become the de-facto streaming standard. This is
reflected for instance by Chisel's built-in `Decoupled` interface type.

Within a single design this is of course not an issue — as long as both the
stream source and sink blocks agree on the same noncompliant interface, the
design will work. However, bearing in mind that there is an increasing number
of independently developed data flow oriented tools, each with their own
standard library, interoperability becomes an issue: whenever a designer needs
to use components from different vendors, they must first ensure that the
interfaces match, and if not, insert the requisite glue logic in between.

A similar issue exists in the software domain, where different programming
languages use different runtime environments and calling conventions. For
instance, efficiently connecting a component written in Java to a component
written in Python requires considerable effort. The keyword here is
"efficiently:" because Java and Python have very different ways of representing
abstract data in memory, one fundamentally has to convert from one
representation to another for any communication between the two components.
This serialization and deserialization overhead can and often does cost more
CPU time than the execution of the algorithms themselves.

The Apache Arrow project attempts to solve this problem by standardizing a way
to represent this abstract data in memory, and providing libraries for popular
programming languages to interact with this data format. The goal is to make
transferring data between two processes as efficient as sharing a pool of
Arrow-formatted memory between them. Arrow also specifies efficient ways of
serializing Arrow data structures for (temporary) storage in files or streaming
structures over a network, and can also be used by GPUs through CUDA. However,
FPGA-based acceleration is at the time of writing missing from the core
project. The Fletcher project attempts to bridge this gap, by providing an
interface layer between the Arrow in-memory format and FPGA accelerators,
presenting the memory to the accelerator in an abstract, tabular form.

In order to represent the complex, nested data types supported by Arrow, the
Fletcher project had to devise its own data streaming format on top of the
de-facto subset of AXI4-stream. Originally, this format was simply a means
to an end, and therefore, not much thought was put into it. Particularly, as
only Arrow-to-device interfaces (and back) were needed for the project, an
interface designed specifically for device-to-device streaming is lacking; in
fact, depending on configuration, the reader and writer interfaces do not even
match completely. Clearly, a standard for streaming complex data types between
components is needed, both within the context of the Fletcher project, and
outside of it.

As far as the writers are aware, no such standard exists as of yet. Defining
such a standard in an open, royalty-free way is the primary purpose of this
document.

Goals
-----

 - Defining a streaming format for complex data types in the context of FPGAs
   and, potentially, ASICs, where "complex data types" include:

    * multi-dimensional sequences;
    * unions (a.k.a. variants);
    * structures such as tuples or records.

 - Doing the above in as broad of a way as possible, without imposing
   unnecessary burdens on the development of simpler components.

 - Allowing for minimization of area and complexity through well-defined
   contracts between source and sink on top of the signal specification itself.

 - Extensibility. This specification should be as usable as possible, even to
   those with use cases not foreseen by this specification.

Non-goals
---------

 - In this specification, a "streaming format" refers to the way in which the
   voltages on a bundle of wires are used to communicate data. We expressly do
   NOT mean streaming over existing communication formats such as Ethernet, and
   certainly not over abstract streams such as POSIX pipes or other
   inter-process communication paradigms. If you're looking for the former,
   have a look at Arrow Flight. The latter is part of Apache Arrow itself
   through their IPC format specification.

 - We do not intend to compete with the AXI4(-stream) specification.
   AXI4-stream is designed for streaming unstructured byte-oriented data;
   OpenTide streams are for streaming structured, complex data types.

 - OpenTide streams have no notion of multi-endpoint network-on-chip-like
   structures. Adding source and destination addressing or other routing
   information can be done through the user signal.

 - The primitive data type in OpenTide is a group of bits. We leave the mapping
   from these bits to higher-order types such as numbers, characters, or
   timestamps to existing specifications.

Document structure
------------------

The remainder of this document consists of a definitions section for
disambiguating the nomenclature used, followed by the specifications for the
three layers of the complete OpenTide specification. These layers are the
physical layer, the primitive layer, and the logical layer. The physical layer
describes the signals that comprise a single stream on a similar level to the
AXI4-stream specification. The primitive layer builds on this by specifying how
primitive objects such as structures and sequences are to be transferred by one
or more physical streams. The logical layer provides mappings for higher-level
data types and transfer methods using the primitive objects defined by the
primitive layer.

Definitions
-----------

While the previous sections attempt to be as unambiguous as possible to
developers with different backgrounds without needing constant cross-reference,
the specification itself requires a more formal approach. This section defines
the nomenclature used in the remainder of this document.

 - *Signal* - a logical, unidirectional wire or bundle of wires in an FPGA
   design, with a single driver and one or more receivers.

    - *Scalar "* - a signal comprised of a single bit, equivalent to an
      `std_logic` signal in VHDL.

    - *Vector "* - a signal comprised of zero or more bits, equivalent to
      an `std_logic_vector` in VHDL. Vector signals have a most significant and
      least significant end, where the most significant end is written first
      and has the highest indices. The least significant bit (if there is one)
      always has index 0.

 - *OpenTide stream* - a bundle of signals used to transfer logical data from
   one source to one sink. OpenTide streams are specified by the OpenTide
   physical layer.

    - *" payload* - the collection of all signals that comprise the stream,
      with the exception of the `valid` and `ready` handshake signals. Further
      subdivided into the data, control, and user signals.

    - *" data signal* - the subset of the stream payload used to transfer
      the actual, logical data carried by the stream.

    - *" data element* - the stream data signal is comprised of one or
      more identical lanes, which can be used to transfer multiple data in a
      single stream transfer. The AXI4-stream equivalent of this concept would
      simply be a byte, as AXI4-stream is byte-oriented.

    - *" data fields* - stream data elements are comprised of zero or more
      data fields, concatenated least-significant-bit-first.

    - *" control signals* - the subset of the stream *payload* used to
      transfer metadata about the organization of the data within this
      transfer.

    - *" user signal* - the subset of the stream payload that is not
      controlled by this specification, i.e. the `user` signal.

    - *" user fields* - the `user` signal is comprised of zero or more user
      fields, concatenated least-significant-bit-first.

    - *" handshake* - the process through which the source and sink of a
      stream agree that the stream payload signals are driven with valid data
      by the source and the acknowledgement of this by the sink, done using the
      `valid` and `ready` signals.

    - *" transfer* - the completion of a single ready/valid handshake,
      causing the stream payload to logically be transferred from source to
      sink.

    - *" packet* - a collection of transfers and elements delimited by a
      nonzero `last` signal driven for the last transfer and a zero `last`
      signal driven for all other transfers.

    - *" batch* - a collection of transfers and elements delimited by a `last`
      signal for which the most significant bit is driven to `'1'` for the last
      transfer, but not for all other transfers. If the stream has zero `last`
      signals ($D=0$), a batch is defined to be equivalent to a single element.

    - *" complexity* ($C$) - a number defining the set of guarantees
      made by the stream source about the structure of the transfers within
      a packet. The higher the number, the less guarantees are made, and the
      simpler the stream. While $C$ is currently comprised of a single integer,
      it may be extended to a period-separated list of integers in the future,
      akin to version numbers.

    - *" dimensionality* ($D$) - the dimensionality of the elements relative
      to a batch. When nonzero, the elements are transferred in depth-first
      order. `last` bit `i` is used to delimit the transfers along dimension
      `i`.

 - *Backpressure* - the means by which a stream sink may block the respective
   source from sending additional transfers. This corresponds to asserting
   `ready` low.

 - *OpenTide River* - a bundle of OpenTide streams used to transfer primitive
   or logical data from one source to one sink. Note that while all data
   streams flow from the source to the sink, control streams may exist that
   flow in the opposite direction.

 - *Streamlet* - a component that operates on one or more OpenTide streams or
   rivers.

Physical layer specification
----------------------------

The physical layer describes the timing and representation of arbitrary data
transferred from a data source to a data sink through a group of signals known
as a stream.

### Parameterization

The signals that belong to an OpenTide stream are uniquely determined by the
following parameters:

 - $N$: the number of data elements in the data signal. Bounds:
   $N \in \mathbb{N}, N \ge 1$.

 - $M$: the bit-width of each data element; that is, the sum of the bit widths
   of the data fields. Bounds: $M \in \mathbb{N}$.

 - $D$: the dimensionality of the elements relative to a batch. Bounds:
   $D \in \mathbb{N}$.

 - $U$: the number of bits in the `user` signal. Bounds: $U \in \mathbb{N}$.

 - $C$: the complexity of the stream, described in the stream complexity
   section below.

$M$ and $D$ together represent the type of data transferred by the stream,
whereas $N$, $U$, and $C$ represent the way in which this data type is
transferred.

### Signal interface requirements

The physical layer defines the following signals.

| Name    | Origin | Width                     | Default   | Complexity condition |
|---------|--------|---------------------------|-----------|----------------------|
| `valid` | Source | *scalar*                  | `'1'`     |                      |
| `ready` | Sink   | *scalar*                  | `'1'`     |                      |
| `data`  | Source | $N \times M$              | all `'0'` |                      |
| `last`  | Source | D                         | all `'1'` |                      |
| `empty` | Source | *scalar*                  | `'0'`     | $C \ge 4$            |
| `stai`  | Source | $\lceil \log_2{N} \rceil$ | 0         | $C \ge 7$            |
| `endi`  | Source | $\lceil \log_2{N} \rceil$ | $N-1$     |                      |
| `strb`  | Source | $N$                       | all `'1'` | $C \ge 8$            |
| `user`  | Source | U                         | all `'0'` |                      |

Streamlets must comply with the following rules for each stream interface.

 - The full name of each signal consists of an optional underscore-terminated
   stream name followed by the name specified above. In case sensitive
   languages, maintaining lowercase is preferable.

 - If a streamlet does not allow a stream parameter to be varied by means of a
   generic, the vector widths can be hardcoded.

 - Signal vectors that are zero-width for the full parameter set supported by
   the streamlet can be omitted. For instance, if a streamlet only supports
   $N=1$, `stai` and `stoi` can be omitted.

 - To allow lower-complexity streams to be connected to higher-complexity
   sinks, all input signals on the interface of the sink must have the default
   values from the table specified in the interface.

 - The signals should be specified in the same order as the table for
   consistency.

### Clock and reset

 - All signals are synchronous to the rising edge of a single clock signal,
   shared between the source and sink.

 - The source and sink of a stream can either share or use different reset
   sources. The requirements on the `valid` and `ready` signals ensure that no
   transfers occur when either the source or the sink is held under reset.

### Complexity

The complexity parameter describes a contractual agreement between the source
and the sink to transfer chunks in a certain way. A higher complexity implies
that the source provides fewer guarantees, and thus that the control logic and
datapath of the sink can make fewer assumptions. Any source with complexity $C$
can be connected to any sink with complexity $C' \ge C$ without glue logic or
loss of functionality; equivalently, increasing the complexity parameter of a
stream is said to be zero-cost (known as the compatibility requirement).

Although only a single natural number suffices for the current OpenTide
specification version, it may in the future consist of multiple
period-separated integers (similar to a version number). The $\ge$ comparison
used in the compatibility requirement is then defined to operate on the
leftmost integer first; if this number is equal, the next integer is compared,
and so on. In the event that one complexity number consists of more integers
than the other, the numbers can be padded with zeros on the right-hand side to
match.

For this version of the specification, the natural numbers 1 through 8 are used
for the complexity number. The following rules are defined based on this
number.

 - $C < 8$: the `strb` signal is always all ones and is therefore omitted.

 - $C < 7$: the `stai` signal is always zero and is therefore omitted.

 - $C < 6$: the `endi` signal is always $N-1$ if `last` is zero. This
   guarantees that the element with index $i$ within the surrounding packet is
   always transferred using data signal lane $i \mod N$.

 - $C < 5$: the `empty` signal is always `'0'` if `last` is all `'0'`, and
   `last` bit $i$ being driven `'1'` implies that `last` bit $i-1$ must also
   be `'1'`. This implies that `last` markers cannot be postponed, even if the
   total number of elements transfered is divisible by $N$.

 - $C < 4$: the `empty` signal is always zero is and is therefore omitted.
   This implies that empty packets are not supported.

 - $C < 3$: once driven `'1'`, the `valid` signal must remain `'1'` until a
   packet-terminating transfer (LSB of the `last` signal is set) is handshaked.

 - $C < 2$: once driven `'1'`, the `valid` signal must remain `'1'` until a
   batch-terminating transfer (MSB of the `last` signal is set) is handshaked.

### Detailed signal description

#### `valid` and `ready`

The `valid` and `ready` signals fulfill the same function as the AXI4-steam
`TVALID` and `TREADY` signals.

 - The source asserts `valid` to `'1'` in the same or a later cycle in which it
   starts driving a valid payload.

 - The state of the payload signals is undefined when `valid` is `'0'`. It is
   recommended for simulation models of streamlets to explicitly set the
   payload signals to `'U'` when this is the case.

 - The sink asserts `ready` to `'1'` when it is ready to consume a stream
   transfer.

 - The source must keep `valid` asserted `'1'` and the payload signals stable
   until the first cycle during which `ready` is also asserted `'1'` by the
   sink.

 - A transfer is considered handshaked when both `valid` and `ready` are
   asserted `'1'` during a clock cycle.

 - The state of the `ready` signal is undefined when `valid` is `'0'`. Sources
   must therefore not wait for `ready` to be asserted `'1'` before asserting
   `valid` to `'1'`. However, sinks *may* wait for `valid` to be `'1'`
   before asserting `ready` to `'1'`; this is up to the implementation.

 - `valid` must be `'0'` while the source is under reset. This prevents
   spurious transfers when the reset of the sink is released before the reset
   of the source.

 - `ready` must be `'0'` while the sink is under reset. This prevents transfers
   from being lost when the reset of the source is released before the reset of
   the sink.

Example timing:

```
            __    __    __    __    __    __    __
 clock  |__/  \__/  \__/  \__/  \__/  \__/  \__/  \_
        |          ___________       ___________
 valid  |_________/          :\_____/    :     :\____
        |                _____       ___________
 ready  |=========._____/    :`====='    :     :`====
        |          ___________       _____ _____
payload |=========<___________>=====<_____X_____>====
                             :           :     :
                             ^           ^     ^
                         stream transfers occur here
```

#### `data`

The `data` signal carries all the data transferred by the stream. It consists
of a flattened array of size $N$ consisting of elements of bit-width $M$.
Within this context, the element subsets of the data vector are also known as
lanes. Each element/lane can be further subdivided into zero or more named
fields.

To ensure compatibility across RTL languages and vendors, the `data` signal is
represented on the streamlet interfaces as a simple vector signal of size
$N \times M$ despite the above. The fields and elements are flattened
element-index-major, LSB-first. Formally, the least significant bit of the
field with index $f$ for lane $l$ is at the following bit position in the
`data` vector:

$l \times M + \sum_{i=0}^{f-1} |F_i|$

where $|F_i|$ denotes the bit-width of field $i$.

Outside of the interfaces of streamlets intended to be connected to streamlets
outside of the designer's control, designers may wish to represent the `data`
signal using an array of records, or a different array indexed by  the lane
index for each field. In the latter case, the signal names should be of the
form `<stream-name>_data_<field-name>` for consistency, and to prevent name
conflicts in future version of this specification.

The following rules apply the `data` signal(s).

 - Element lane $i$ of the `data` signal is don't-care in any of the following
   cases:

    - `valid` is `'0'`;
    - `empty` is `'1'`;
    - $i$ > `endi`;
    - $i$ < `stai`; or
    - bit $i$ of `strb` is `'0'`.

 - Element lane $i$ carries significant data during a transfer if and only if:

    - `empty` is `'0'`;
    - $i$ <= `endi`;
    - $i$ >= `stai`; and
    - bit $i$ of `strb` is `'1'`.

#### `last`

The `last` signal is a $D$-bit vector, wherein bit $i$ being driven `'1'` marks
that the associated transfer terminates enumeration of elements across
dimension $i$ in the current batch. Intuitively, a stream with $D=2$ and $N=1$
can transfer the batch represented by `[[1, 2], [3, 4, 5]]` with the following
transfers:

| `data` | `last` |
|--------|--------|
| 1      | `"00"` |
| 2      | `"01"` |
| 3      | `"00"` |
| 4      | `"00"` |
| 5      | `"11"` |

The following rules apply.

 - The `last` signal is don't-care while `valid` is `'0'`.

 - The LSB of the `last` vector is used to terminate packets. When `D=0`,
   packets reduce to single elements.

 - The MSB of the `last` vector is used to terminate batches. When `D=0`,
   batches reduce to single elements.

 - While not named, any intermediate `last` bits terminate the intermediate
   dimensions of a batch.

 - It is illegal to terminate dimension `i` without also terminating dimension
   `i=1`. Intuitively, violating this would encode an inner list that extends
   beyond the list it is an element of. Therefore, in transfers where `empty`
   is not asserted, the `last` vector must be a thermometer code. For example,
   for `D=3`, only the following values are valid: `"000"`, `"001"`, `"011"`,
   and `"111"`.

 - The `empty` flag can be used to delay termination of a dimension. In this
   case, the `last` value need not always be thermometer code. For instance,
   a transfer with `last` = `"001"` followed by a transfer with `last` =
   `"110"` and empty driven `'1'` is a legal way to terminate batch. However,
   each dimension must only be terminated once, and must be terminated in inner
   to outer order. For instance, `last` = `"010"` followed by `last` = `"101"`
   is illegal because the order is violated. `last` = `"001"` with `empty` =
   `'0'` followed by `last` = `"111"` with `empty` = `'1'` is legal, but
   encodes an empty packet before the batch is closed.

#### `empty`

The `empty` signal is used to encode empty packets, and to delay transfer of
dimension boundary information when such information is not known during the
last transfer containing actual data.

 - The `empty` signal is don't-care while `valid` is `'0'`.

 - When `empty` is asserted, only control and user-specified information is
   transferred. The `data`, `stai`, `endi`, and `strb` signals are therefore
   don't-care.

#### `stai` and `endi`

For streams that can carry more than one element per cycle ($N > 1$), the
`stai` (start index) and `endi` (end index) signals encode how many and which
of the data element lanes contain valid data. They are vectors of length
$\lceil \log_2{N} \rceil$, interpreted as unsigned integers. The following
rules apply.

 - The `stai` and `endi` signals are don't-care while `valid` is `'0'` or
   while `empty` is `'1'`.

 - `stai` must always be less than or equal to `endi`.

 - `endi` must always be less than `N`.

#### `strb`

For streams that can carry more than one element per cycle ($N > 1$), the
`strb` signal can be used to enable or disable specific element data lanes.
It is a vector of size $N$. A `strb` signal being `'0'` implies that the
respective lane does *not* carry significant data. Otherwise, its significance
is determined by the `stai`, `endi`, and `empty` signals.

It is obvious that the `strb` signal can be used to describe everything that
`stai`, `endi`, and `empty` can together describe and more, so they may appear
redundant. Refer to the "`strb` vs. `stai`/`endi`" section for the reasoning
behind including all four of these signals in the specification.

The `strb` signal is don't-care while `valid` is `'0'` or while `empty` is
`'1'`.

#### `user`

The `user` signal carries user-defined control information; that is,
information associated with a transfer rather than a data element. It can be
subdivided into zero or more user fields. The stream parameter $U$ must be set
to the total bit-width of these user fields.

To ensure compatibility across RTL languages and vendors, the `user` signal is
represented on the streamlet interfaces as a simple vector signal of size
$U$ despite the above. The user fields are concatenated LSB-first.

Outside of the interfaces of streamlets intended to be connected to streamlets
outside of the designer's control, designers may wish to represent the `user`
signal using a record or an individual signal for each field. In the latter
case, the signal names should be of the form `<stream-name>_user_<field-name>`
for consistency, and to prevent name conflicts in future version of this
specification.

The following rules apply the `user` signal(s).

 - The `user` signal is don't-care while `valid` is `'0'`.

 - Streamlets that transform a stream purely element-wise or merely buffer a
   stream should allow for a generic-configurable `user` signal, even if they
   do not themselves use the `user` signal.

 - Streamlets that do manipulate transfers should simply not support the `user`
   signals beyond those they specify themselves.

### `strb` vs. `stai`/`endi`

At first glance, the `strb` signal appears to make `stai`, `endi`, and `empty`
redundant, as the `strb` signal on its own can describe any lane utilization
that can be described through those signals and more. The fact that AXI4-stream
uses `TSTRB` (and `TKEEP`) for this purpose lends further credence to this
thought. We nevertheless specify `stai`, `endi`, and `empty` for the following
reasons.

 - Many data sources by design output on consecutive lanes, and thus have no
   need for a full `strb` signal. This is typically the case, for instance,
   for a streamlet that reads from a consecutive memory region using a wide
   memory interface bus.

 - Simple streamlets that manipulate a stream usually operate on a lane-by-lane
   basis, and therefore do not affect the lane layout they receive on their
   input. Streamlets that maintain state may also need an enable bit per lane.
   Having more than just the `strb` bit to interpret is a downside here;
   however, generating these lane enable signals is an efficient operation on
   6-LUT FPGAs up to $N=64$, requiring only two levels of logic with three LUTs
   per lane.

 - Many data sinks, for instance those that write back to memory, can benefit
   from the guarantee that only consecutive lanes are used. This reduces the
   complexity of the control logic in particular. Most importantly, without
   `strb`, this number of input bits to the control logic has complexity
   $\mathcal{O}(\log N)$ versus $\mathcal{O}(N)$ and is therefore much more
   likely to be efficiently synthesizable with few levels of logic, reducing
   area and increasing frequency or decreasing the necessary pipeline depth.

 - Avoiding the `strb` signals reduces interconnect and therefore congestion
   for wide streams by a small amount.

In summary, a full `strb` signal is often not needed, while it increases
hardware complexity even for simple streamlets operating on wide streams. Since
wide streams are fundamental to achieving performance competitive to CPU/GPU on
an FPGA, and primitive operations often are simple, optimizing for these cases
is important. Therefore, the `strb` signal is used only for streams with
$C \ge 8$, the highest complexity level currently defined.

Sources that need the `strb` signal will usually drive `stai`, `endi`, and
`empty` with their default values, giving full control to the `strb` signal.
The only exception is for signalling empty packets and postponing `last`
markers — this *must* be done by asserting `empty`. However, the `stai` and
`endi` signals must still be present for sinks with $C \ge 8$ such that they
can also support sources with $C < 8$.

It is worth noting that in this case the `stai` and `endi` signals will most
likely be removed by the synthesizer during constant propagation, at least
until the first FIFO is encountered; tools may not be capable of propagating
across a FIFO due to the memory that sits in between. Regardlessly, the small
fraction of systems that require `strb` will likely be of such complexity that
any overhead induced by `stai` and `endi` is negligible.

### Arrays of streams

Streamlets that take an indexable array of streams as input or output can do
so by individually concatenating the stream signals into vectors, ordered
LSB-first. For instance, an array of three streams will have a `valid` signal
vector of width three, and so on.

Primitive layer specification
-----------------------------

This layer specifies how a group of OpenTide streams, known as a river, can be
used to transfer complex nested types. The "primitive" in the name refers to
the fact that such nested types are typically primitives in higher-order
data-oriented languages.

Looking back to the parameters of OpenTide streams, this layer only specifies
the values for parameters $M$ and $D$. $M$ is described by way of a list of
fields, each with their own bit width. The remaining parameters are independent
of the data transferred over the streams, and must be specified independently
by the designer, based on the performance/area/complexity considerations of
their design.

### Type representation

We first specify a generic type system that describes exactly the set of
primitive types supported by rivers.

#### Intuitive description

A river allows transferrence of data of types recursively defined using the
following primitives:

 - a value of a certain bit-width;
 - sequences of some subtype with variable length (representing lists, arrays,
   and so on);
 - structures of a number of subtypes with optionally named fields (also known
   as (named) tuples and records in some languages); and
 - unions of a number of subtypes with optionally named fields (also known as
   variants in some languages).

Some flexibility is provided in how these types can be represented. This is
intended to give the hardware designer the freedom to determine which
representation is the most suitable for their streamlet or application. The
logical specification, if used, intends to impose more strict requirements on
these representations to increase the odds of two independently developed
streamlets sharing the same interface where applicable.

To handle sequences in particular, we define the notion of domains. A domain
is essentially defined to be a group of one or more physical streams that are
guaranteed to carry the same "shape" of data at runtime in terms of nested
sequences. For instance, if a stream were to convey the nested structure
$[[1, 2], [3, 4, 5]]$, all streams in the same domain are guaranteed to carry
data of the form $[[a, b], [c, d, e]]$, while streams in a different domain may
for instance carry data of the form $[[f, g, h, i], [j]]$; the $D$ parameter
for these streams is the same, but their data flows cannot logically be merged.

Domains form a tree-like structure, where each added dimension adds a branch.
For instance, both the previously mentioned domains may have the same parent
domain, wherein the streams transfer data of the form $[k, l]$. Using the type
notation we will define more formally later, this structure would result from
for instance `[([T], [U])]`.

We also define a special kind of domain, called a flattening domain. Such a
domain bears no significance in the logical sense; it has the same logical
dimensionality as its parent domain, and the data carried by it also has the
same shape. However, the $D$ parameter of the physical streams belonging to
this domain is reset to 0; they essentially carry flattened data. The shape of
the data can only be recovered by copying the shape over from a stream in the
parent domain, or by reconstructing it through some other means.

Flattening domains may seem like an odd concept at first glance, but turn out
to be useful in practice, for instance to describe a river consisting of a
length stream and a flattened data stream (this is called a `Vec` in the
logical layer). Such structures are particularly important in the context of
wide streams (large $N$) carrying many short sequences; without flattening,
each transfer can fundamentally carry at most one sequence, while the flattened
`Vec` representation does not have this limitation. Without flattening domains,
the only way to represent such a river would be to support multiple root
domains, in which case the shape information would be lost entirely.

#### Formal description

We define river $R$ as an ordered tree of so-called domains. Each domain $X$
is a triple of an identifier, a tuple containing zero or more streams, and a
flattening flag:

$X = ( I_X, ( S_0, S_1, \cdots , S_{n-1} ), f )$

Each stream is furthermore described by an identifier, a set of data fields,
and a reverse-direction flag:

$S = ( I_S, ( F_0, F_1, \cdots , F_{n-1} ) , r )$

Finally, each field is described by an identifier and a bit-width:

$F = ( I_F, n )$

A river maps to one or more physical OpenTide streams by means of preorder
depth-first traversal of the domains, concatenating the streams encountered.
The $M$ parameter of each stream is defined by the sum of the bit-widths of
its fields. The $D$ parameter is determined by counting the number of domains
that need to be traversed to get to either the root domain or a domain with
the flattening flag set, such that a stream belonging to the root domain or
a domain with the flattening flag set has $D = 0$. The remaining parameters do
not relate to the type carried by the stream, and can thus be freely chosen by
the designer.

### Type construction

We define two sets of equivalent grammars to recursively describe or construct
the type of a river, one intended to be human readable/writable, and one that
uses only case-insensitive alphanumerical characters and underscores such that
it can be embedded into an identifier. The latter serves a similar purpose as
name mangling does in C++: to allow generative tools to embed type information
in identifiers in a reproducible and nonambiguous way.

Let us first define the human-readable grammar with EBNF syntax, where
`positive` represents a positive integer with regular expression
`/[1-9][0-9]*/`, and `identifier` represents an identifier with regular
expression `[a-zA-Z_][a-zA-Z0-9_]*`.

```ebnf
(* functional rules *)
bitfield    = "b" , positive ;
sequence    = "[" , river , "]" ;
flatten     = "-" , river , "-" ;
bundle      = "|" , [ reversibles ] , "|" ;
tuple       = "(" , [ rivers ] , ")" ;
union       = "{" , [ rivers ] , "}" ;
null-union  = "{" , "0" , [ "," , rivers ] , "}" ;
named       = identifier , ":" , river ;

(* toplevel rule *)
river       = bitfield   | sequence | flatten | union
            | null-union | tuple    | bundle  | named ;

(* helper rules *)
rivers      = river , { "," , river } , [ "," ] ;
reversible  = [ "^" ] , river ;
reversibles = reversible , { "," , reversible } , [ "," ] ;
```

The name-mangling representation has the same rules, but uses letters in place
of symbols and a slightly simplified syntax. To disambiguate between letters
that carry grammatical meaning and identifiers, the identifiers are
underscore-terminated, and underscores in the user-specified identifier must
be replaced with a double underscore.

```ebnf
(* functional rules *)
bitfield    = positive ;
sequence    = "s" , river ;
flatten     = "f" , river ;
bundle      = "b" , [ reversibles ] , "e" ;
tuple       = "t" , [ rivers ] , "e" ;
union       = "u" , [ rivers ] , "e" ;
null-union  = "n" , [ rivers ] , "e" ;
named       = "_" , identifier , "_" , river ;

(* toplevel rule *)
river       = bitfield   | sequence | flatten | union
            | null-union | tuple    | bundle  | named ;

(* helper rules *)
rivers      = river , { "c" , river } ;
reversible  = [ "r" ] , river ;
reversibles = reversible , { "c" , reversible } ;
```

The following sections describe the semantics of the functional rules.

#### Bitfield

```ebnf
bitfield = "b" , positive ;
```

A bitfield represents any higher-level datatype that can be represented with a
nonzero, fixed number of bits. What kind of value is represented by the
bitfield and how is out of the scope of this layer of the specification.

The described river consists of a single domain $X$, with

$X = ( \varnothing{}, ( S_0 ), 0 )$

$S_0 = ( \varnothing{}, ( F_0 ) , 0 )$

$F_0 = ( \varnothing{}, n )$

where $n$ is the positive number of bits defined by the rule.

#### Sequence

```ebnf
sequence = "[" , river , "]" ;
```

The sequence operator transforms a river data type into a sequence thereof by
adding a domain.

The described river consists of a new root domain $X$, with

$X = ( \varnothing{}, \varnothing{}, 0 )$

The domain tree from the child type is added as a child of $X$.

#### Flatten

```ebnf
flatten = "-" , river , "-" ;
```

The flattening operator indicates that the child river type is a flattened
representation of a new root domain. That is, all `last` flags of the root
domain are removed. Note that this operator is functionally no-op unless it is
the descendant of a sequence operator; that is, the domain added by this
operator does not *remain* the root.

If the root domain of the child type is not flattened (its $f = 0$), the
described river consists of a new root domain $X$, with

$X = ( \varnothing{}, \varnothing{}, 1 )$

The domain tree from the child type is then added as a child of $X$.

If the root domain of the child type is flattened (its $f = 1$), the operator
is no-op; that is, the domain tree of the child tree is returned without
modification.

#### Bundle

```ebnf
reversible  = [ "^" ] , river ;
reversibles = reversible , { "," , reversible } , [ "," ] ;
bundle      = "|" , [ reversibles ] , "|" ;
```

The bundling operator combines a number of child river types together into one.
The logical datatype equivalent for this is a structure, tuple, or record, so
the root domains of the child types are combined together. In the physical
representation, the streams of the subrivers remain separated, so the transfers
that constitute the logical structure can occur in different cycles.

The direction of one or more of the subrivers can be reversed. This allows the
sink to send back control information. For instance, a source may grant random
access to a large vector to a sink by allowing the sink to send it a stream of
indices, to which the source then replies with the data. When reversed streams
are involved, it is of exceptional importance that the inter-stream
dependencies (specified later) are adhered to. Summarizing those dependencies
briefly; if stream $S$ is listed before stream $S'$ in the type, stream $S$
acts as a command stream for stream $S'$.

The described river consists of root domain $X$, which is constructed by
merging the root domains of the child types $X_{0..n-1}$ into one. If the root
of all the child types have the same value for $f$ (the flattening flag), the
following holds:

$X = ( \varnothing{}, \prod\limits_{i=0}^{n-1} S_{X_i}, f_{X_0} )$

where $\prod$ signifies concatenation. If there are both child domains with
$f = 0$ and $f = 1$, the following holds:

$X = ( \varnothing{}, \prod\limits_{i=0}^{n-1} \left\{\begin{matrix} S_{X_i} & f_{X_i} = 0 \\ \varnothing & f_{X_i} = 1 \end{matrix}\right. , 0 )$

$X' = ( \varnothing{}, \prod\limits_{i=0}^{n-1} \left\{\begin{matrix} \varnothing & f_{X_i} = 0 \\ S_{X_i} & f_{X_i} = 1 \end{matrix}\right. , 1 )$

where $X$ is the new root domain, and $X'$ is a child domain thereof.

In both cases, the descendent domains of the child types are not merged
together, and become descendents of the domain their roots were merged into.

If a child type is reversed (by means of a caret prefix), the $r$ flag for
all its streams is inverted.

If the bundle operator has zero types as its input, it returns a null river,
consisting only of root domain $X_\varnothing$, defined as

$X_\varnothing = ( \varnothing{}, \varnothing{}, 0 )$

#### Tuple

```ebnf
tuple = "(" , [ rivers ] , ")" ;
```

The tuple operator is very similar to the bundling operator. It only differs in
the physical representation of the first stream of each child type: for a tuple
these are merged together, while for a bundle they remain separate streams. The
primary child streams $S_{X_{0..n-1}},0$ are merged into stream $S_{X,0}$ as
follows:

$S_{X,0} = ( \varnothing{}, \prod\limits_{i=0}^{n-1} \left\{\begin{matrix} \varnothing & |S_{X_i}| = 0 \vee r_{X_i,0} = 1 \\ F_{X_i,0} & |S_{X_i}| > 0 \wedge r_{X_i,0} = 0 \end{matrix}\right. , 0 )$

That is, the field tuple of the merged stream is the concatenation of the field
tuples of the first stream in the root domain of each child data type that has
at least one stream in the root domain and this first stream is not reversed.

The remaining streams and domains of the child types are represented as they
would be in a bundle.

Note that it is possible to get a stream with zero fields this way. While it
does not carry data, its handshake and control signals may still be relevant
for the streamlets, therefore it is not pruned.

If the tuple operator has zero types as its input, it returns a river with a
single domain $X$, satisfying

$X = ( \varnothing{}, ( S_\varnothing ), 0 )$

where

$S_\varnothing = ( \varnothing{}, \varnothing{}, 0 )$

#### Union

```ebnf
union      = "{" , [ rivers ] , "}" ;
null-union = "{" , "0" , [ "," , rivers ] , "}" ;
```

The union operators combine a number of child river types together into one;
however, unlike tuples and bundles, only one of the child data types is valid
at a time. The logical datatype equivalent for this is a union, option, or
variant. Which data type is valid for each element is signified by an option ID
field.

...

The remaining streams and domains of the child types are represented as they
would be in a bundle. However, **TODO**




#### Named

```ebnf
named = identifier , ":" , river ;
```



### Physical representation of river types




#### Inter-stream dependencies










# Random notes follow, WIP


b<width>    -> bit field with the given width
[T]         -> increase the dimensionality of T (surround T with a normal domain)
-T-         -> flatten T (surround T with a flattening domain)
<prefix>: T -> add a prefix to all identifiers
{T,U,...}   -> union of T, U, ...
{0,T,U,...} -> union of null, T, U, ...
(T,U,...)   -> tuple of T, U, ... represented by data fields in a single stream where possible
|T,^U,...|  -> tuple of T, U, ... represented by a different stream for each field, possibly in reverse direction



an ordered set of dimensions $X_i$. A dimension consists of a reference to
a parent dimension $p_i$ (which can be $\varnothing$), zero or more streams
$S_{i_j}$, and a flattening flag $f_i$. That is:

$X_i = \{ p_i, S_{i_j}, f_i \}$

Within this context, each stream is defined by an ordered set of fields









The types are described recursively
through a set of operators. These are:

 - the structure operator;
 - the union operator;
 - the sequence operator;
 - the bundle operator.

We also specify two grammars for representing these operators, one that is
intended to be human-readable, and one that can be described within an
identifier (case-insensitive alphanumeric plus underscore) to serve a similar
purpose as C++ type/name mangling.

Before we can describe the operators, we need to define a representation for
them to operate on.






We define that a river $R$ consists of
one or more streams $S_i$ and one or more domains $G_j$:

$R = \{ S_{0..|S|-1}, G_{0..|G|-1} \}$

In this context, each stream consists of the names and bit widths of zero or
more data fields $F_j$, a name for the stream itself, reverse-direction flag
$R$, and domain index $g$. Ignoring the name tags, a stream is therefore
defined as

$S_i = \{ F_{i_j}, R_i, g_i \}$

A domain consists of a parent stream index $p$, which can also be
$\varnothing$, a dimensionality $D$, and a flattening flag $F$. It is therefore
defined as

$G_i = \{ p_i, D_i, F_i \}$

The dimensionality and element width of $S_i$ are defined as follows:

$D_{S_i} = \sum_{f \in F_i} |f|$

$M_{S_i} = D_{g_i}

where $|f|$ denotes the bit-width of field $f$.

The following definitions and conditions apply in addition:

 - $S_0$ is defined to be the primary stream, while $S_i$ for $i \ge 1$ are
   defined to be the secondary streams.

 - $G_0$ is defined to be the primary domain, while $G_i$ for $i \ge 1$ are
   defined to be the secondary domains.

 - The primary stream must be part of the primary domain, i.e. $g_0 = 0$.

 - The primary domain is the only domain to have no parent stream, i.e.
   $p_0 = \varnothing$ and $p_i \ne \varnothing$ for $i \ge 1$.

 - The directed graph formed by the $g$ and $p$ indices forms a tree of
   domains, of which the primary domain is the root. That is, the graph can
   not contain any cycles.




or more domains, each containing one or more streams. The first stream in the
first domain is called the primary stream ($S_0,0$), the remaining streams in
the first domain are called secondary streams ($S_0,j$), and all the streams in
the remaining domains are called tertiary stream ($S_i,j$). The significance of
domains is that the dimensionality of all the secondary streams in a domain
relate to the dimensionality of their primary stream
