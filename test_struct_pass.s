	.file	"test_struct_pass.5eaa250f28e196a-cgu.0"
	.section	.text._ZN16test_struct_pass4main17h136e5d7e360e8296E,"ax",@progbits
	.hidden	_ZN16test_struct_pass4main17h136e5d7e360e8296E
	.globl	_ZN16test_struct_pass4main17h136e5d7e360e8296E
	.p2align	4
	.type	_ZN16test_struct_pass4main17h136e5d7e360e8296E,@function
_ZN16test_struct_pass4main17h136e5d7e360e8296E:
	.cfi_startproc
	subq	$88, %rsp
	.cfi_def_cfa_offset 96
	movq	$4096, 8(%rsp)
	leaq	16(%rsp), %rax
	leaq	18(%rsp), %rcx
	leaq	20(%rsp), %rdx
	leaq	22(%rsp), %rsi
	movabsq	$4295163909, %rdi
	movq	%rdi, 16(%rsp)
	movq	%rax, 24(%rsp)
	movq	_ZN4core3fmt3num3imp52_$LT$impl$u20$core..fmt..Display$u20$for$u20$u16$GT$3fmt17h01e7c848a01e956dE@GOTPCREL(%rip), %rax
	movq	%rax, 32(%rsp)
	movq	%rcx, 40(%rsp)
	movq	%rax, 48(%rsp)
	movq	%rdx, 56(%rsp)
	movq	%rax, 64(%rsp)
	movq	%rsi, 72(%rsp)
	movq	%rax, 80(%rsp)
	leaq	.Lanon.63e1393d9e911302138e66b8ca36ffc5.0(%rip), %rdi
	leaq	24(%rsp), %rsi
	callq	*_ZN3std2io5stdio6_print17h9af62a1472ff7e83E@GOTPCREL(%rip)
	addq	$88, %rsp
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end0:
	.size	_ZN16test_struct_pass4main17h136e5d7e360e8296E, .Lfunc_end0-_ZN16test_struct_pass4main17h136e5d7e360e8296E
	.cfi_endproc

	.section	.text._ZN3std2rt10lang_start17hda7f98d1bc4dc47dE,"ax",@progbits
	.hidden	_ZN3std2rt10lang_start17hda7f98d1bc4dc47dE
	.globl	_ZN3std2rt10lang_start17hda7f98d1bc4dc47dE
	.p2align	4
	.type	_ZN3std2rt10lang_start17hda7f98d1bc4dc47dE,@function
_ZN3std2rt10lang_start17hda7f98d1bc4dc47dE:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	movl	%ecx, %r8d
	movq	%rdx, %rcx
	movq	%rsi, %rdx
	movq	%rdi, (%rsp)
	leaq	.Lanon.63e1393d9e911302138e66b8ca36ffc5.1(%rip), %rsi
	movq	%rsp, %rdi
	callq	*_ZN3std2rt19lang_start_internal17h9f282d832ae47dd5E@GOTPCREL(%rip)
	popq	%rcx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end1:
	.size	_ZN3std2rt10lang_start17hda7f98d1bc4dc47dE, .Lfunc_end1-_ZN3std2rt10lang_start17hda7f98d1bc4dc47dE
	.cfi_endproc

	.section	".text._ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h1ddb75bb9983aa7eE","ax",@progbits
	.p2align	4
	.type	_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h1ddb75bb9983aa7eE,@function
_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h1ddb75bb9983aa7eE:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	movq	(%rdi), %rdi
	callq	_ZN3std3sys9backtrace28__rust_begin_short_backtrace17hceeab7076d09704bE
	xorl	%eax, %eax
	popq	%rcx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end2:
	.size	_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h1ddb75bb9983aa7eE, .Lfunc_end2-_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h1ddb75bb9983aa7eE
	.cfi_endproc

	.section	.text._ZN3std3sys9backtrace28__rust_begin_short_backtrace17hceeab7076d09704bE,"ax",@progbits
	.p2align	4
	.type	_ZN3std3sys9backtrace28__rust_begin_short_backtrace17hceeab7076d09704bE,@function
_ZN3std3sys9backtrace28__rust_begin_short_backtrace17hceeab7076d09704bE:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	callq	*%rdi
	#APP
	#NO_APP
	popq	%rax
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end3:
	.size	_ZN3std3sys9backtrace28__rust_begin_short_backtrace17hceeab7076d09704bE, .Lfunc_end3-_ZN3std3sys9backtrace28__rust_begin_short_backtrace17hceeab7076d09704bE
	.cfi_endproc

	.section	".text._ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h8821324f00becd5dE","ax",@progbits
	.p2align	4
	.type	_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h8821324f00becd5dE,@function
_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h8821324f00becd5dE:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	movq	(%rdi), %rdi
	callq	_ZN3std3sys9backtrace28__rust_begin_short_backtrace17hceeab7076d09704bE
	xorl	%eax, %eax
	popq	%rcx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end4:
	.size	_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h8821324f00becd5dE, .Lfunc_end4-_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h8821324f00becd5dE
	.cfi_endproc

	.section	.text.main,"ax",@progbits
	.globl	main
	.p2align	4
	.type	main,@function
main:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	movq	%rsi, %rcx
	movslq	%edi, %rdx
	leaq	_ZN16test_struct_pass4main17h136e5d7e360e8296E(%rip), %rax
	movq	%rax, (%rsp)
	leaq	.Lanon.63e1393d9e911302138e66b8ca36ffc5.1(%rip), %rsi
	movq	%rsp, %rdi
	xorl	%r8d, %r8d
	callq	*_ZN3std2rt19lang_start_internal17h9f282d832ae47dd5E@GOTPCREL(%rip)
	popq	%rcx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end5:
	.size	main, .Lfunc_end5-main
	.cfi_endproc

	.type	.Lanon.63e1393d9e911302138e66b8ca36ffc5.0,@object
	.section	.rodata.str1.1,"aMS",@progbits,1
.Lanon.63e1393d9e911302138e66b8ca36ffc5.0:
	.asciz	"\tdeclared=\300\n, written=\300\024, parent_0_declared=\300\023, parent_0_written=\300\001\n"
	.size	.Lanon.63e1393d9e911302138e66b8ca36ffc5.0, 69

	.type	.Lanon.63e1393d9e911302138e66b8ca36ffc5.1,@object
	.section	.data.rel.ro..Lanon.63e1393d9e911302138e66b8ca36ffc5.1,"aw",@progbits
	.p2align	3, 0x0
.Lanon.63e1393d9e911302138e66b8ca36ffc5.1:
	.asciz	"\000\000\000\000\000\000\000\000\b\000\000\000\000\000\000\000\b\000\000\000\000\000\000"
	.quad	_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h8821324f00becd5dE
	.quad	_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h1ddb75bb9983aa7eE
	.quad	_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h1ddb75bb9983aa7eE
	.size	.Lanon.63e1393d9e911302138e66b8ca36ffc5.1, 48

	.ident	"rustc version 1.94.0 (4a4ef493e 2026-03-02)"
	.section	".note.GNU-stack","",@progbits
