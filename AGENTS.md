To help you complete tasks, you make look-up code in the library. It's available in the folder ./ngx-rust. It is never to be changed and committed. Our module will still use the version published on crates.io. Keep in mind that the local clone may be more up-to-date than the available library.

You may also look into the nginx doc in ./nginx-doc.md (obtained from https://nginx.org/en/docs/dev/development_guide.html).

We don't care about backward-compatibility between changes. However, please keep the ready accurate and up-to-date.

I'm a never-nester, so try avoiding high amounts of nested code blocks. I will tolerate until 3 levels. I have nothing against long functions though.

I dislike the fully qualified rust syntax (like crate::log or hibernate::spawn_idle_monitor). Make sure to import stuff instead. Use a prelude for crate imports.
