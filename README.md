license-hound is a tool to help sniffing out licenses from all crate
dependencies.

While this tool attempts to help you with fulfilling legal obligations, it
should not be relied upon to do so successfully and no author or contributor
can be held accountable for the results of its use. See
[LICENSE](https://github.com/maghoff/cargo-license-hound/blob/master/LICENSE)
for the full terms of use and limitation of liability clause.

What does it do?
================
license-hound attempts to locate the LICENSE files of all crate dependencies
of a rust project so you can successfully comply with the conditions. It
tries the following:

 1. Look in the downloaded crate for filenames that could be correct. (All
    published crates [ought to](https://github.com/rust-lang/cargo/issues/3537)
    include a license file)
 2. If not found, and the source repository is on GitHub, ask the
    [GitHub license API](https://developer.github.com/v3/licenses/)
 3. If still not found, attempt to retrieve a LICENSE file via HTTPS
    requests to GitHub

The filenames license-hound looks for are variants seen in the wild, including
typos. (LICENSE, COPYING, LICENSE.txt, and so on)

license-hound will attempt to find license files for the MIT, BSD-3-Clause and
MPL-2.0 licenses, in that order. Other licenses are not accepted by
license-hound, because it was not necessary for its initial use case.

license-hound was written specifically to find the license files for the
dependencies of [Sausagewiki](https://github.com/maghoff/sausagewiki) and may
or may not work for your use case.

How do I try it?
================
    cargo install --git https://github.com/maghoff/cargo-license-hound

and then, from your project directory:

    cargo license-hound > license-hound.json

It prints out a compact JSON report of its findings. It is best to store this
to a file for further processing.

The report is a list of JSON objects, one for each dependency. Each entry
includes some metadata as well as a `"conclusion"`, which details the license
findings.

----

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
SOFTWARE.
