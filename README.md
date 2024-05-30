# boomhammer: small HTTP load testing tool

boomhammer is similar to `wrk` in that it is intended to be very simple. It does not perform as well as `wrk`, but improving performance is a goal: right now it's around 75% of what `wrk` does.

## Usage

```
boomhammer -c <cpus> <url>
```

As of this writing, the URL must be an IP address.

# Performance

Local nginx on all threads of a Ryzen 5900X:

wrk:

```
% wrk -c 1000 http://127.0.0.1:8000
Running 10s test @ http://127.0.0.1:8000
  2 threads and 1000 connections
  Thread Stats   Avg      Stdev     Max   +/- Stdev
    Latency     3.09ms  487.05us  20.51ms   87.19%
    Req/Sec    82.41k     5.03k  106.84k    86.22%
  1626627 requests in 10.05s, 1.29GB read
Requests/sec: 161821.94
Transfer/sec:    131.64MB
```

boomhammer (2 cpus only):

```
Elapsed: 10s 1ms 446us 83ns - Stats { successes: 1014474, failures: 13 } 101447 r/s
```

## Development Status

boomhammer is extremely immature and is used to stress another product I'm working on. As time passes, more features will be added and performance will be improved.

## Author

Erik Hollensbe <erik@hollensbe.org>

## License

MIT
