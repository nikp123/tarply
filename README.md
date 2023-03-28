What is this?
-------------

A tool that applies only differences between .tar files.

Imagine it as a rdiff but for being able to process tar files directly.

Current state
-------------

 - Seems mostly fine in simple tests that I've done
 - DO NOT use a directory already filled with files
 - HAS NOT BEEN THOUGHLY TESTED YET (DONT RELY ON THIS YET)
 - DO NOT change ignore rules after the first backup or you'll end up with
 erroneous files afterwards.
 - THERE IS NO FILE HEADER FOR THE FOLDER STATE AND IT WILL BE BROKEN IN AN
 FUTURE UPDATE.
 - I only just got to some rust concepts, so the average rustacian may scream at
 me for the horrors that I've brought to their programming language.

Why is this a thing?
--------------------

This exists because some cloud service/webhosting providers don't offer SSH
or any other sane means of transfering backups across servers. Instead, they
just offer ``.tar.gz`` backups that are pain to deal with.

Why are they a pain? 

Glad you've asked. On every backup I have to extract the entire tar archive
just so I'm able to notice what files were actually changed. This is extremely
wasteful as I have to waste my precious disk write cycles on this pointless task.

So instead I wrote this. This can completely work in UNIX pipes, which makes it
extremely flexible.

How does it do it?
------------------

The tool is designed to apply partial updates of your target folder using the
``.tar`` files provided by your web host. It achieves this by reading the tar
file on the fly, calculating hashes and keeping track of the file structure
inside of a state file.

Any differences of the state compared to the previous one is followed up by a
write to said folder. As long as changes remain small, it will be able to apply
changes using the as least disk writes as reasonably achieveable.

The state file is then further compressed for more savings.

How do I?
---------

1. Install rust.
2. ``cargo run``.
3. The rest is self-explanitory.

Loisence
--------

[here is moi televishun loisence, innit bruv](LICENSE)

