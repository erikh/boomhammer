# boomhammer: small HTTP load testing tool

boomhammer is similar to `wrk` in that it is intended to be very simple. It does not perform as well as `wrk`, but improving performance is a goal: right now it's around 75% of what `wrk` does.

## Usage

```
boomhammer -c <cpus> <url>
```

As of this writing, the URL must be an IP address.

## Development Status

boomhammer is extremely immature and is used to stress another product I'm working on. As time passes, more features will be added and performance will be improved.

## Author

Erik Hollensbe <erik@hollensbe.org>

## License

MIT
