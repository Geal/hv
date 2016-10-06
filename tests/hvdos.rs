//! Example hypervisor and 16 bits VM from https://github.com/mist64/hvdos/blob/master/hvdos.c
//! original blog post at http://www.pagetable.com/?p=764
//! guest VM code taken from https://lwn.net/Articles/658511/
#![feature(alloc,heap_api)]
extern crate hv;
extern crate libc;
extern crate alloc;

use hv::ffi;
use hv::vmc::{VMCS,VMXReason};
use libc::{uint32_t,uint64_t};
use std::iter::repeat;
use std::os::raw::c_void;
use std::io::Write;
use alloc::heap;
use std::slice;

pub fn rreg(vcpu: ffi::hv_vcpuid_t, reg: ffi::hv_x86_reg_t) -> uint64_t {
  let mut v: uint64_t = 0;

  unsafe {
    let res = ffi::hv_vcpu_read_register(vcpu, reg, &mut v);
    if res != 0 {
      panic!("rreg res: {}", res);
    }
  }
  return v;
}

/* write GPR */
pub fn wreg(vcpu: ffi::hv_vcpuid_t, reg: ffi::hv_x86_reg_t, v: uint64_t) {
  unsafe {
    let res = ffi::hv_vcpu_write_register(vcpu, reg, v);
    if res != 0 {
      panic!("wreg res: {}", res);
    }
  }
}

/* read VMCS field */
pub fn rvmcs(vcpu: ffi::hv_vcpuid_t, field: uint32_t) -> uint64_t {
  let mut v: uint64_t = 0;

  unsafe {
    let res = ffi::hv_vmx_vcpu_read_vmcs(vcpu, field, &mut v);
    if res != 0 {
      panic!("rvcms res: {}", res);
    }
  }

  return v;
}

/* write VMCS field */
pub fn wvmcs(vcpu: ffi::hv_vcpuid_t, field: uint32_t, v: uint64_t) {
  unsafe {
    let res = ffi::hv_vmx_vcpu_write_vmcs(vcpu, field, v);
    if res != 0 {
      panic!("wvcms res: {}", res);
    }
  }
}

/* desired control word constrained by hardware/hypervisor capabilities */
pub fn cap2ctrl(cap: uint64_t, ctrl: uint64_t) -> uint64_t {
(ctrl | (cap & 0xffffffff)) & (cap >> 32)
}


#[test]
fn vm_create() {
  unsafe {
    let mut res = ffi::hv_vm_create(ffi::VMOptions::HV_VM_DEFAULT as u64);
    if res != 0 {
      panic!("vm create res: {}", res);
    }

    let mut vmx_cap_pinbased: uint64_t = 0;
    let mut vmx_cap_procbased : uint64_t = 0;
    let mut vmx_cap_procbased2: uint64_t = 0;
    let mut vmx_cap_entry: uint64_t = 0;

    res = ffi::hv_vmx_read_capability(ffi::hv_vmx_capability_t::HV_VMX_CAP_PINBASED, &mut vmx_cap_pinbased);
    if res != 0 {
      panic!("vmx read capability res: {}", res);
    }
    res = ffi::hv_vmx_read_capability(ffi::hv_vmx_capability_t::HV_VMX_CAP_PROCBASED, &mut vmx_cap_procbased);
    if res != 0 {
      panic!("vmx read capability res: {}", res);
    }
    res = ffi::hv_vmx_read_capability(ffi::hv_vmx_capability_t::HV_VMX_CAP_PROCBASED2, &mut vmx_cap_procbased2);
    if res != 0 {
      panic!("vmx read capability res: {}", res);
    }
    res = ffi::hv_vmx_read_capability(ffi::hv_vmx_capability_t::HV_VMX_CAP_ENTRY, &mut vmx_cap_entry);
    if res != 0 {
      panic!("vmx read capability res: {}", res);
    }
    println!("capabilities: pinbased: {} procbased: {} procbased2: {} entry: {}", vmx_cap_pinbased, vmx_cap_procbased,
      vmx_cap_procbased2, vmx_cap_entry);

    let capacity: usize = 4 * 1024;
    let mem_raw = heap::allocate(capacity, 4096);

    //let mut mem = Vec::with_capacity(capacity);
    //mem.extend(repeat(0).take(capacity));

    println!("allocating memory at {:?}", mem_raw);
    //map the vec at address 0
    res = ffi::hv_vm_map(mem_raw as *mut c_void, 0, capacity,
      ffi::Enum_Unnamed4::HV_MEMORY_READ as u64  |
      ffi::Enum_Unnamed4::HV_MEMORY_WRITE as u64 |
      ffi::Enum_Unnamed4::HV_MEMORY_EXEC as u64);
    if res != 0 {
      panic!("vm map res: {}", res);
    }

    let mem = slice::from_raw_parts_mut(mem_raw, capacity);

    let mut vcpu: ffi::hv_vcpuid_t = 0;

    res = ffi::hv_vcpu_create(&mut vcpu, ffi::Enum_Unnamed3::HV_VCPU_DEFAULT as u64);
    if res != 0 {
      panic!("vcpu create res: {}", res);
    }

    println!("vcpu id: {}", vcpu);

    let VMCS_PRI_PROC_BASED_CTLS_HLT: u64 = 1 << 7;
    let VMCS_PRI_PROC_BASED_CTLS_CR8_LOAD: u64 = 1 << 19;
    let VMCS_PRI_PROC_BASED_CTLS_CR8_STORE: u64 = 1 << 20;

    /* set VMCS control fields */
    wvmcs(vcpu, VMCS::VMCS_CTRL_PIN_BASED as u32, cap2ctrl(vmx_cap_pinbased, 0));
    wvmcs(vcpu, VMCS::VMCS_CTRL_CPU_BASED as u32, cap2ctrl(vmx_cap_procbased,
                                                   VMCS_PRI_PROC_BASED_CTLS_HLT |
                                                   VMCS_PRI_PROC_BASED_CTLS_CR8_LOAD |
                                                   VMCS_PRI_PROC_BASED_CTLS_CR8_STORE));
    wvmcs(vcpu, VMCS::VMCS_CTRL_CPU_BASED2 as u32, cap2ctrl(vmx_cap_procbased2, 0));
    wvmcs(vcpu, VMCS::VMCS_CTRL_VMENTRY_CONTROLS as u32, cap2ctrl(vmx_cap_entry, 0));
    wvmcs(vcpu, VMCS::VMCS_CTRL_EXC_BITMAP as u32, 0xffffffff);
    wvmcs(vcpu, VMCS::VMCS_CTRL_CR0_MASK as u32, 0x60000000);
    wvmcs(vcpu, VMCS::VMCS_CTRL_CR0_SHADOW as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_CTRL_CR4_MASK as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_CTRL_CR4_SHADOW as u32, 0);
    /* set VMCS guest state fields */
    wvmcs(vcpu, VMCS::VMCS_GUEST_CS as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_CS_LIMIT as u32, 0xffff);
    wvmcs(vcpu, VMCS::VMCS_GUEST_CS_AR as u32, 0x9b);
    wvmcs(vcpu, VMCS::VMCS_GUEST_CS_BASE as u32, 0);

    wvmcs(vcpu, VMCS::VMCS_GUEST_DS as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_DS_LIMIT as u32, 0xffff);
    wvmcs(vcpu, VMCS::VMCS_GUEST_DS_AR as u32, 0x93);
    wvmcs(vcpu, VMCS::VMCS_GUEST_DS_BASE as u32, 0);

    wvmcs(vcpu, VMCS::VMCS_GUEST_ES as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_ES_LIMIT as u32, 0xffff);
    wvmcs(vcpu, VMCS::VMCS_GUEST_ES_AR as u32, 0x93);
    wvmcs(vcpu, VMCS::VMCS_GUEST_ES_BASE as u32, 0);

    wvmcs(vcpu, VMCS::VMCS_GUEST_FS as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_FS_LIMIT as u32, 0xffff);
    wvmcs(vcpu, VMCS::VMCS_GUEST_FS_AR as u32, 0x93);
    wvmcs(vcpu, VMCS::VMCS_GUEST_FS_BASE as u32, 0);

    wvmcs(vcpu, VMCS::VMCS_GUEST_GS as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_GS_LIMIT as u32, 0xffff);
    wvmcs(vcpu, VMCS::VMCS_GUEST_GS_AR as u32, 0x93);
    wvmcs(vcpu, VMCS::VMCS_GUEST_GS_BASE as u32, 0);

    wvmcs(vcpu, VMCS::VMCS_GUEST_SS as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_SS_LIMIT as u32, 0xffff);
    wvmcs(vcpu, VMCS::VMCS_GUEST_SS_AR as u32, 0x93);
    wvmcs(vcpu, VMCS::VMCS_GUEST_SS_BASE as u32, 0);

    wvmcs(vcpu, VMCS::VMCS_GUEST_LDTR as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_LDTR_LIMIT as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_LDTR_AR as u32, 0x10000);
    wvmcs(vcpu, VMCS::VMCS_GUEST_LDTR_BASE as u32, 0);

    wvmcs(vcpu, VMCS::VMCS_GUEST_TR as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_TR_LIMIT as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_TR_AR as u32, 0x83);
    wvmcs(vcpu, VMCS::VMCS_GUEST_TR_BASE as u32, 0);

    wvmcs(vcpu, VMCS::VMCS_GUEST_GDTR_LIMIT as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_GDTR_BASE as u32, 0);

    wvmcs(vcpu, VMCS::VMCS_GUEST_IDTR_LIMIT as u32, 0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_IDTR_BASE as u32, 0);

    wvmcs(vcpu, VMCS::VMCS_GUEST_CR0 as u32, 0x20);
    wvmcs(vcpu, VMCS::VMCS_GUEST_CR3 as u32, 0x0);
    wvmcs(vcpu, VMCS::VMCS_GUEST_CR4 as u32, 0x2000);


    let mut code:Vec<u8> = vec!(
      0xba, 0xf8, 0x03, /* mov $0x3f8, %dx */
      0x00, 0xd8,       /* add %bl, %al */
      0x04, '0' as u8,  /* add $'0', %al */
      0xee,             /* out %al, (%dx) */
      0xb0, '\n' as u8, /* mov $'\n', %al */
      0xee,             /* out %al, (%dx) */
      0x90,0x90,0x90,0x90,0x90,0x90,0x90,0x90,0x90,0x90,0x90,0x90,0x90,0x90,0x90,
      0xf4              /* hlt */
    );

    (&mut mem[256..]).write(&code);

    /* set up GPRs, start at adress 0x100 */
    wreg(vcpu, ffi::hv_x86_reg_t::HV_X86_RIP, 0x100);

    wreg(vcpu, ffi::hv_x86_reg_t::HV_X86_RFLAGS, 0x2);
    wreg(vcpu, ffi::hv_x86_reg_t::HV_X86_RSP, 0x0);

    /* set up args for addition */
    wreg(vcpu, ffi::hv_x86_reg_t::HV_X86_RAX, 0x5);
    wreg(vcpu, ffi::hv_x86_reg_t::HV_X86_RBX, 0x3);

    let mut chars = 0u8;
    loop {
      let mut run_res = ffi::hv_vcpu_run(vcpu);
      if run_res != 0 {
        panic!("vcpu run res: {}", run_res);
      }

      let exit_reason = rvmcs(vcpu, VMCS::VMCS_RO_EXIT_REASON as u32);
      println!("exit reason: {}", exit_reason);

      let rip = rreg(vcpu, ffi::hv_x86_reg_t::HV_X86_RIP);
      println!("RIP at {}", rip);

      if exit_reason == VMXReason::VMX_REASON_IRQ as u64 {
        println!("IRQ");
      } else if exit_reason == VMXReason::VMX_REASON_HLT as u64 {
          println!("HALT");
          break;
      } else if exit_reason == VMXReason::VMX_REASON_EPT_VIOLATION as u64 {
          println!("EPT VIOLATION, ignore");
          //break;
      } else if exit_reason == VMXReason::VMX_REASON_IO as u64 {
        println!("IO");
        if chars > 2 {
          panic!("the guest code should not return more than 2 chars on the serial port");
        }
        let qual = rvmcs(vcpu, VMCS::VMCS_RO_EXIT_QUALIFIC as u32);
        if (qual >> 16) & 0xFFFF == 0x3F8 {
          let rax = rreg(vcpu, ffi::hv_x86_reg_t::HV_X86_RAX);
          println!("RAX == {}", rax);
          println!("got char: {}", (rax as u8) as char);

          if chars == 0 {
            assert_eq!(rax, '8' as u64);
          }
          if chars == 1 {
            assert_eq!(rax, '\n' as u64);
          }
          chars += 1;


          let inst_length = rvmcs(vcpu, VMCS::VMCS_RO_VMEXIT_INSTR_LEN as u32);

          wreg(vcpu, ffi::hv_x86_reg_t::HV_X86_RIP, rip + inst_length);
        } else {
          println!("unrecognized IO port, exit");
          break;
        }
        /*
        let rax = rreg(vcpu, ffi::hv_x86_reg_t::HV_X86_RAX);
        println!("RAX == {}", rax);
        let rdx = rreg(vcpu, ffi::hv_x86_reg_t::HV_X86_RDX);
        println!("RDX == {}", rdx);
        //println!("address 0x3f8: {:?}", &mem[0x3f8..0x408]);
        println!("qual: {}", qual);
        let size = qual >> 62;
        println!("size: {}", size);
        let direction = (qual << 2) >> 63;
        println!("direction (0=out): {}, {}", direction, qual & 0x8);
        let string = (qual << 4)    >> 63;
        println!("string (1=string): {}, {}", string, qual &0x10);
        println!("port: {}", (qual >> 16) & 0xFFFF);
        */
      }
    }

    res = ffi::hv_vcpu_destroy(vcpu);
    if res != 0 {
      panic!("vcpu destroy res: {}", res);
    }
    res = ffi::hv_vm_unmap(0, mem.len());
    if res != 0 {
      panic!("vm unmap res: {}", res);
    }

    heap::deallocate(mem_raw, capacity, 4096);
  }

  //assert!(false);
}
