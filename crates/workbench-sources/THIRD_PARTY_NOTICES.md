# Third-party protocol references

The metadata protocol constants, endpoint names and field contracts were
independently implemented in Rust using the fixed references in README.md.
No downloader, media reconstruction, retry middleware or filesystem execution
code was copied into this crate.

JMComic-Crawler-Python: Copyright (c) 2023 hect0x7.
jmcomic-downloader and picacomic-downloader: Copyright (c) 2024-2026 lanyeeee (https://github.com/lanyeeee).
PicaComic-Api historical thumbnail host references: Copyright (c) 2019 FlanSR.

MIT License

Permission is hereby granted, free of charge, to any person obtaining a copy
of this software and associated documentation files (the "Software"), to deal
in the Software without restriction, including without limitation the rights
to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
copies of the Software, and to permit persons to whom the Software is
furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in
all copies or substantial portions of the Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN
THE SOFTWARE.

Supplementary account-only reference: Miuzarte/PicaComic-go, fixed commit
25d20c875b69c94f7980fad8d8d5b06c7ef3d1cb, Apache License 2.0. Only the request
route/method and response field facts were consulted; no Go implementation
was copied or adapted. Its license is available at:
https://github.com/Miuzarte/PicaComic-go/blob/25d20c875b69c94f7980fad8d8d5b06c7ef3d1cb/LICENSE
