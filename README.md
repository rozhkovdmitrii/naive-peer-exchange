## Naive Peer Exchange (`naive-peer`)

The  **`naive-peer`** binary is designed as a node of a  decentralized peer network. It's a simple p2p gossiping application written in Rust. Once connected, the peer should send a random gossip message to all the other peers every N seconds.  The messaging period is also specified as a command line parameter.  When a peer receives a message from the other peers, it prints it  in the console.

## Building

**`naive-peer`** can be built to the `bin` directory using the cargo manager as follows:

```sh
cargo build --release --out-dir bin -Z unstable-options
```

## Testing

There are two options to test naive peer exchange functionality. The former one is by using the unit tests are supplied with the project, these tests check if the predefined peers (up to 20 units) are fully connected to each other since these are started:

```sh
cargo test
...
running 3 tests
test test_couple_peers_network ... ok
test test_a_few_peers_network ... ok
test test_many_peers_network ... ok
...
```

Tests could also be processed separatly:

```sh
cargo test --package naive_p2p_lib test_couple_peers_network 
...
test test_couple_peers_network ... ok
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 2 filtered out; finished in 0.02s
```

The latter way to test whether peers are connected  or properly disconnected - is manually using utilities like `lsof`, `wireshark`. This behavior aspects  could be difficult to check from inside so. It can be done by running a few instances of **`navie-peer`** in parallel and playing with them:

```sh
bin/naive-peer -P 20 
# 00:00:00 - My address is: 0.0.0.0:8080
# 00:00:02 - Connected to the peer at: 127.0.0.1:8081
# 00:00:08 - Connected to the peer at: 127.0.0.1:8082
```

```sh
bin/naive-peer -P 45 -p 8081 -c "127.0.0.1:8080"
# 00:00:00 - My address is: 0.0.0.0:8081
# 00:00:00 - Connected to the peer at: 127.0.0.1:8080
# 00:00:00 - Connected to the peer at: 127.0.0.1:8082
```

```sh
bin/naive-peer -P 60 -p 8082 -c "127.0.0.1:8081"
# 00:00:00 - My address is: 0.0.0.0:8082
# 00:00:00 - Connected to the peer at: 127.0.0.1:8081
# 00:00:00 - Connected to the peer at: 127.0.0.1:8080
# 00:00:17 - Received message [BhGkNWbA1g] from: 127.0.0.1:8080
```

```sh
watch -n 0.1 sudo lsof -iTCP -cnaive-peer -a -nP

COMMAND     PID    USER   FD   TYPE DEVICE SIZE/OFF NODE NAME
naive-pee 88995 rozhkov    6u  IPv4 559535      0t0  TCP *:8080 (LISTEN)
naive-pee 88995 rozhkov    7u  IPv4 564529      0t0  TCP 127.0.0.1:8080->127.0.0.1:59754 (ESTABLISHED)
naive-pee 88995 rozhkov    8u  IPv4 556724      0t0  TCP 127.0.0.1:8080->127.0.0.1:59770 (ESTABLISHED)
naive-pee 89002 rozhkov    6u  IPv4 560603      0t0  TCP *:8081 (LISTEN)
naive-pee 89002 rozhkov    7u  IPv4 560604      0t0  TCP 127.0.0.1:59754->127.0.0.1:8080 (ESTABLISHED)
naive-pee 89002 rozhkov    8u  IPv4 568343      0t0  TCP 127.0.0.1:8081->127.0.0.1:51348 (ESTABLISHED)
naive-pee 89005 rozhkov    6u  IPv4 565423      0t0  TCP *:8082 (LISTEN)
naive-pee 89005 rozhkov    7u  IPv4 565424      0t0  TCP 127.0.0.1:51348->127.0.0.1:8081 (ESTABLISHED)
naive-pee 89005 rozhkov    8u  IPv4 565425      0t0  TCP 127.0.0.1:59770->127.0.0.1:8080 (ESTABLISHED)
```

It is also possible to interrupt connections from inside using **`gdb`** and **`shutdown`** system call and see how it affects on the network under testing

```sh
sudo gdb -p 89005 --batch --ex 'call shutdown(7, 0)'
```

## Configuration

Currently the **`naive-peer`** configuration is trivially simple and expressively determined by the help of it. It provides long and short keys, as well as default values that are applied when they are not present.
```sh
bin/naive-peer --help
Provides a peer exchange that imples a naive messaging functionality

Usage: naive-peer [OPTIONS]

Options:
  -P, --period <PERIOD>    Messaging period [default: 2]
      --address <ADDRESS>  Public address to be sent as a peer address [default: 127.0.0.1]
  -p, --port <PORT>        Port of the current peer to listen to [default: 8080]
  -c, --connect <CONNECT>  Address of the peer to connect to initially
  -h, --help               Print help
  -V, --version            Print version
```

## Example

The main functionality of **`naive-peer`** is messaging, so an example is shown below. It demonstrates that messages are received from nodes at times determined by the period, 45 and 60 sec. settings set there, respectively.
```sh
bin/naive-peer -P 20 
# 00:00:00 - My address is: 0.0.0.0:8080
# 00:00:01 - Connected to the peer at: 127.0.0.1:8081
# 00:00:02 - Connected to the peer at: 127.0.0.1:8082
# 00:00:46 - Received message [BqfDqjAZ67] from: 127.0.0.1:8081
# 00:01:02 - Received message [rFaZKKhIXs] from: 127.0.0.1:8082
# 00:01:31 - Received message [24XvdWlmRI] from: 127.0.0.1:8081
# 00:02:02 - Received message [36ieDzzBHY] from: 127.0.0.1:8082
# 00:02:16 - Received message [24DbLiut5L] from: 127.0.0.1:8081
# 00:03:01 - Received message [q6ZQ6TSqwj] from: 127.0.0.1:8081
# 00:03:02 - Received message [J86SoKPF4n] from: 127.0.0.1:8082
```

## Future improvements

1. **DNS names** - using of DNS names could affect on exchange negatively and this functionality should be improved.
2. **Negotiation** - naive exchange could be destructed by the malicious people that can connect to the nodes and intentionally send wrong peering information to the nodes to puzzle its interconnecting so these scenarios should be concerned and prevented. Presumably it could be done by proving that the node is actually available at the given public address.
3. **Connection sustainability** - current implementation stops messaging and drop the connection if protocol errors happen or connection is broken the related peer is dropped away. In this situation it's better to try to reconnect to the peer.
4. **Improve scalability** - current implementation's connections number is limited by connecting each to all the others. This number was tested and it certainly due to be improved during the further research on system limitation, time rates etc. There are also a way to message each to the all others on the not fully connected peer exchange so the way to reduce a number of simultaneous connections to be concerned.



