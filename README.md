# divvun-worker-speller

## Building

This must be built using `just` due to complex linking requirements.

```
just build-macos
# or
just build-linux
```

## Usage

```
divvun-worker-speller path/to/some.zhfst
```

## Configuration

Environment variables:

`HOST` and `PORT` are read for determining which host and port to use. Defaults to `localhost:4000`.