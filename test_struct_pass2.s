	.file	"test_struct_pass2.fb319918a38c4433-cgu.0"
	.section	.text._ZN17test_struct_pass24done17hef2a608d2f827913E,"ax",@progbits
	.p2align	4
	.type	_ZN17test_struct_pass24done17hef2a608d2f827913E,@function
_ZN17test_struct_pass24done17hef2a608d2f827913E:
	.cfi_startproc
	movq	(%rsi), %rax
	movq	%rax, (%rdi)
	movq	8(%rsi), %rax
	movq	%rax, 8(%rdi)
	retq
.Lfunc_end0:
	.size	_ZN17test_struct_pass24done17hef2a608d2f827913E, .Lfunc_end0-_ZN17test_struct_pass24done17hef2a608d2f827913E
	.cfi_endproc

	.section	.text._ZN17test_struct_pass24main17hb7b8428789075556E,"ax",@progbits
	.hidden	_ZN17test_struct_pass24main17hb7b8428789075556E
	.globl	_ZN17test_struct_pass24main17hb7b8428789075556E
	.p2align	4
	.type	_ZN17test_struct_pass24main17hb7b8428789075556E,@function
_ZN17test_struct_pass24main17hb7b8428789075556E:
	.cfi_startproc
	pushq	%rbx
	.cfi_def_cfa_offset 16
	subq	$112, %rsp
	.cfi_def_cfa_offset 128
	.cfi_offset %rbx, -16
	movq	$4096, 16(%rsp)
	movabsq	$4295098373, %rax
	movq	%rax, 24(%rsp)
	leaq	96(%rsp), %rbx
	leaq	16(%rsp), %rsi
	movq	%rbx, %rdi
	callq	_ZN17test_struct_pass25entry17hf8db66dd769aed31E
	movq	%rsp, %rdi
	movq	%rbx, %rsi
	callq	_ZN17test_struct_pass24done17hef2a608d2f827913E
	leaq	8(%rsp), %rax
	leaq	10(%rsp), %rcx
	leaq	12(%rsp), %rdx
	leaq	14(%rsp), %rsi
	movq	%rax, 32(%rsp)
	movq	_ZN4core3fmt3num3imp52_$LT$impl$u20$core..fmt..Display$u20$for$u20$u16$GT$3fmt17h01e7c848a01e956dE@GOTPCREL(%rip), %rax
	movq	%rax, 40(%rsp)
	movq	%rcx, 48(%rsp)
	movq	%rax, 56(%rsp)
	movq	%rdx, 64(%rsp)
	movq	%rax, 72(%rsp)
	movq	%rsi, 80(%rsp)
	movq	%rax, 88(%rsp)
	leaq	.Lanon.f11ea02d6a31032881b7db4a360f291e.0(%rip), %rdi
	leaq	32(%rsp), %rsi
	callq	*_ZN3std2io5stdio6_print17h9af62a1472ff7e83E@GOTPCREL(%rip)
	addq	$112, %rsp
	.cfi_def_cfa_offset 16
	popq	%rbx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end1:
	.size	_ZN17test_struct_pass24main17hb7b8428789075556E, .Lfunc_end1-_ZN17test_struct_pass24main17hb7b8428789075556E
	.cfi_endproc

	.section	.text._ZN17test_struct_pass25entry17hf8db66dd769aed31E,"ax",@progbits
	.p2align	4
	.type	_ZN17test_struct_pass25entry17hf8db66dd769aed31E,@function
_ZN17test_struct_pass25entry17hf8db66dd769aed31E:
	.cfi_startproc
	movq	(%rsi), %rax
	movzwl	8(%rsi), %ecx
	movzwl	10(%rsi), %edx
	incl	%edx
	movq	%rax, (%rdi)
	movw	%cx, 8(%rdi)
	movw	%dx, 10(%rdi)
	movl	12(%rsi), %eax
	movl	%eax, 12(%rdi)
	retq
.Lfunc_end2:
	.size	_ZN17test_struct_pass25entry17hf8db66dd769aed31E, .Lfunc_end2-_ZN17test_struct_pass25entry17hf8db66dd769aed31E
	.cfi_endproc

	.section	.text._ZN3std2rt10lang_start17he0f61a1e46fd5fe0E,"ax",@progbits
	.hidden	_ZN3std2rt10lang_start17he0f61a1e46fd5fe0E
	.globl	_ZN3std2rt10lang_start17he0f61a1e46fd5fe0E
	.p2align	4
	.type	_ZN3std2rt10lang_start17he0f61a1e46fd5fe0E,@function
_ZN3std2rt10lang_start17he0f61a1e46fd5fe0E:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	movl	%ecx, %r8d
	movq	%rdx, %rcx
	movq	%rsi, %rdx
	movq	%rdi, (%rsp)
	leaq	.Lanon.f11ea02d6a31032881b7db4a360f291e.1(%rip), %rsi
	movq	%rsp, %rdi
	callq	*_ZN3std2rt19lang_start_internal17h9f282d832ae47dd5E@GOTPCREL(%rip)
	popq	%rcx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end3:
	.size	_ZN3std2rt10lang_start17he0f61a1e46fd5fe0E, .Lfunc_end3-_ZN3std2rt10lang_start17he0f61a1e46fd5fe0E
	.cfi_endproc

	.section	".text._ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h3b181c8f8cd4bcb3E","ax",@progbits
	.p2align	4
	.type	_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h3b181c8f8cd4bcb3E,@function
_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h3b181c8f8cd4bcb3E:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	movq	(%rdi), %rdi
	callq	_ZN3std3sys9backtrace28__rust_begin_short_backtrace17h62c68687da4b8321E
	xorl	%eax, %eax
	popq	%rcx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end4:
	.size	_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h3b181c8f8cd4bcb3E, .Lfunc_end4-_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h3b181c8f8cd4bcb3E
	.cfi_endproc

	.section	.text._ZN3std3sys9backtrace28__rust_begin_short_backtrace17h62c68687da4b8321E,"ax",@progbits
	.p2align	4
	.type	_ZN3std3sys9backtrace28__rust_begin_short_backtrace17h62c68687da4b8321E,@function
_ZN3std3sys9backtrace28__rust_begin_short_backtrace17h62c68687da4b8321E:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	callq	*%rdi
	#APP
	#NO_APP
	popq	%rax
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end5:
	.size	_ZN3std3sys9backtrace28__rust_begin_short_backtrace17h62c68687da4b8321E, .Lfunc_end5-_ZN3std3sys9backtrace28__rust_begin_short_backtrace17h62c68687da4b8321E
	.cfi_endproc

	.section	".text._ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h68a38e9a19875150E","ax",@progbits
	.p2align	4
	.type	_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h68a38e9a19875150E,@function
_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h68a38e9a19875150E:
	.cfi_startproc
	pushq	%rax
	.cfi_def_cfa_offset 16
	movq	(%rdi), %rdi
	callq	_ZN3std3sys9backtrace28__rust_begin_short_backtrace17h62c68687da4b8321E
	xorl	%eax, %eax
	popq	%rcx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end6:
	.size	_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h68a38e9a19875150E, .Lfunc_end6-_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h68a38e9a19875150E
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
	leaq	_ZN17test_struct_pass24main17hb7b8428789075556E(%rip), %rax
	movq	%rax, (%rsp)
	leaq	.Lanon.f11ea02d6a31032881b7db4a360f291e.1(%rip), %rsi
	movq	%rsp, %rdi
	xorl	%r8d, %r8d
	callq	*_ZN3std2rt19lang_start_internal17h9f282d832ae47dd5E@GOTPCREL(%rip)
	popq	%rcx
	.cfi_def_cfa_offset 8
	retq
.Lfunc_end7:
	.size	main, .Lfunc_end7-main
	.cfi_endproc

	.type	.Lanon.f11ea02d6a31032881b7db4a360f291e.0,@object
	.section	.rodata.str1.1,"aMS",@progbits,1
.Lanon.f11ea02d6a31032881b7db4a360f291e.0:
	.asciz	"\tdeclared=\300\n, written=\300\024, parent_0_declared=\300\023, parent_0_written=\300\001\n"
	.size	.Lanon.f11ea02d6a31032881b7db4a360f291e.0, 69

	.type	.Lanon.f11ea02d6a31032881b7db4a360f291e.1,@object
	.section	.data.rel.ro..Lanon.f11ea02d6a31032881b7db4a360f291e.1,"aw",@progbits
	.p2align	3, 0x0
.Lanon.f11ea02d6a31032881b7db4a360f291e.1:
	.asciz	"\000\000\000\000\000\000\000\000\b\000\000\000\000\000\000\000\b\000\000\000\000\000\000"
	.quad	_ZN4core3ops8function6FnOnce40call_once$u7b$$u7b$vtable.shim$u7d$$u7d$17h68a38e9a19875150E
	.quad	_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h3b181c8f8cd4bcb3E
	.quad	_ZN3std2rt10lang_start28_$u7b$$u7b$closure$u7d$$u7d$17h3b181c8f8cd4bcb3E
	.size	.Lanon.f11ea02d6a31032881b7db4a360f291e.1, 48

	.ident	"rustc version 1.94.0 (4a4ef493e 2026-03-02)"
	.section	".note.GNU-stack","",@progbits
