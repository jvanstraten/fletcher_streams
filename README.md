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
      signals ($D=0$), a batch is defined to equivalent to a single element.

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

 - $C \lt 8$: the `strb` signal is always all ones and is therefore omitted.

 - $C \lt 7$: the `stai` signal is always zero and is therefore omitted.

 - $C \lt 6$: the `endi` signal is always $N-1$ if `last` is zero. This
   guarantees that the element with index $i$ within the surrounding packet is
   always transferred using data signal lane $i \mod N$.

 - $C \lt 5$: the `empty` signal is always `'0'` if `last` is all `'0'`, and
   `last` bit $i$ being driven `'1'` implies that `last` bit $i-1$ must also
   be `'1'`. This implies that `last` markers cannot be postponed, even if the
   total number of elements transfered is divisible by $N$.

 - $C \lt 4$: the `empty` signal is always zero is and is therefore omitted.
   This implies that empty packets are not supported.

 - $C \lt 3$: once driven `'1'`, the `valid` signal must remain `'1'` until a
   packet-terminating transfer (LSB of the `last` signal is set) is handshaked.

 - $C \lt 2$: once driven `'1'`, the `valid` signal must remain `'1'` until a
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




# TODO from here onwards



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

