# 🚇 publictransport

[![Travis](https://img.shields.io/travis/kiliankoe/publictransport.svg?style=flat-square)](https://travis-ci.org/kiliankoe/publictransport)

#### *Work in Progress*

This library aims to be a high-level abstraction to the APIs of several public transport providers. It's currently nothing more than an idea though and will likely keep on being a playground for quite some time. 

The idea - feasible or not, time will tell - is to write adapters for several kinds of public transport systems (EFA, Hafas, TRIAS, Navitia, plain GTFS etc.), combine that with a bunch of providers implementing those systems and arrive at a library capable of talking to these providers and accessing whatever data these systems provide, be it the finding of nearby stops, routing from one stop to another or whatever. 

Not wanting to invest the time and effort to implement and more importantly, maintain something like this for several different languages/platforms, the core library and adapters will be implemented in Rust, but offering a C API which can then be used by thin FFI-based wrappers. In my head at least that seems like a good idea and I'm really hoping it works out 🙈😅

## Roadmap

WIP

## Contributors

Kilian Koeltzsch, [@kiliankoe](https://github.com/kiliankoe)

## License

publictransport is available under the MIT license. See the LICENSE file for more info.
