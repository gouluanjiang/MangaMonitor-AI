# Reader source reuse

`model.ts:isReaderNeighbor` adapts the `eagerLoad` predicate from Komga's
`komga-webui/src/components/readers/PagedReader.vue` at commit
`65981e600edb24944ffaae4818ff2716a5fa08dd` (release 1.27.1):
https://github.com/gotson/komga/blob/65981e600edb24944ffaae4818ff2716a5fa08dd/komga-webui/src/components/readers/PagedReader.vue

The Vue carousel and server routes are not embedded. Reader state, native IPC,
position persistence, virtualization and byte-bounded cache are application code.
Suwayomi-WebUI (MPL-2.0) and Yomikiru (MIT) were evaluated as interaction/source
references; no code from either is included. Their complete reader components
depend on their own stores, requests and Electron/server contracts.

## Komga MIT License

Copyright (c) 2019 Gauthier Roebroeck

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all
copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
