# Getting started

We'll create a new cargo directory and add DarkWow to our `Cargo.toml`,
like so:

```
{{#include ../../../../../example/dchat/dchatd/Cargo.toml:darkwow}}
```

Be sure to replace the path to DarkWow with the correct path for your
setup.

Once that's done we can access DarkWow's net methods inside of
dchat. We'll need a few more external libraries too, so add these
dependencies:

```
{{#include ../../../../../example/dchat/dchatd/Cargo.toml:dependencies}}
```


