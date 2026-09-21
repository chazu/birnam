static void* __birnam_resumable_step_0(void* machine, void* const* args, unsigned long n){
if(n!=1UL)return birnam_nil();
long pc=birnam_smallIntValue(args[0]);
switch(pc){
case 0L:{
void* __birnam_put_selector_9=birnam_intern("oopAt:put:");
void* __birnam_put_slot_9=MKV(5L);
void* __birnam_value_9=birnam_send(machine,birnam_intern("oopAt:"),(void*[]){MKV(1L)},1L);
void* __birnam_put_args_9[]={__birnam_put_slot_9,__birnam_value_9};
(void)birnam_send(machine,__birnam_put_selector_9,__birnam_put_args_9,2L);
void* __birnam_put_selector_10=birnam_intern("oopAt:put:");
void* __birnam_put_slot_10=MKV(6L);
void* __birnam_value_10=birnam_intern("probeDouble");
void* __birnam_put_args_10[]={__birnam_put_slot_10,__birnam_value_10};
(void)birnam_send(machine,__birnam_put_selector_10,__birnam_put_args_10,2L);
return birnam_send(machine,birnam_intern("callSite:"),(void*[]){MKV(0L)},1L);
}
case 1L:{
void* __birnam_put_selector_3=birnam_intern("oopAt:put:");
void* __birnam_put_slot_3=MKV(0L);
void* __birnam_value_3=birnam_send(machine,birnam_intern("oopAt:"),(void*[]){MKV(2L)},1L);
void* __birnam_put_args_3[]={__birnam_put_slot_3,__birnam_value_3};
(void)birnam_send(machine,__birnam_put_selector_3,__birnam_put_args_3,2L);
void* __birnam_put_selector_4=birnam_intern("oopAt:put:");
void* __birnam_put_slot_4=MKV(1L);
void* __birnam_value_4=birnam_send(machine,birnam_intern("oopAt:"),(void*[]){MKV(0L)},1L);
void* __birnam_put_args_4[]={__birnam_put_slot_4,__birnam_value_4};
(void)birnam_send(machine,__birnam_put_selector_4,__birnam_put_args_4,2L);
void* __birnam_put_selector_5=birnam_intern("oopAt:put:");
void* __birnam_put_slot_5=MKV(3L);
void* __birnam_value_5=birnam_global("Probe");
void* __birnam_put_args_5[]={__birnam_put_slot_5,__birnam_value_5};
(void)birnam_send(machine,__birnam_put_selector_5,__birnam_put_args_5,2L);
void* __birnam_put_selector_9=birnam_intern("oopAt:put:");
void* __birnam_put_slot_9=MKV(5L);
void* __birnam_value_9=birnam_send(machine,birnam_intern("oopAt:"),(void*[]){MKV(3L)},1L);
void* __birnam_put_args_9[]={__birnam_put_slot_9,__birnam_value_9};
(void)birnam_send(machine,__birnam_put_selector_9,__birnam_put_args_9,2L);
void* __birnam_put_selector_10=birnam_intern("oopAt:put:");
void* __birnam_put_slot_10=MKV(6L);
void* __birnam_value_10=birnam_intern("probeEffect:alias:");
void* __birnam_put_args_10[]={__birnam_put_slot_10,__birnam_value_10};
(void)birnam_send(machine,__birnam_put_selector_10,__birnam_put_args_10,2L);
void* __birnam_put_selector_11=birnam_intern("oopAt:put:");
void* __birnam_put_slot_11=MKV(7L);
void* __birnam_value_11=birnam_send(machine,birnam_intern("oopAt:"),(void*[]){MKV(0L)},1L);
void* __birnam_put_args_11[]={__birnam_put_slot_11,__birnam_value_11};
(void)birnam_send(machine,__birnam_put_selector_11,__birnam_put_args_11,2L);
void* __birnam_put_selector_12=birnam_intern("oopAt:put:");
void* __birnam_put_slot_12=MKV(8L);
void* __birnam_value_12=birnam_send(machine,birnam_intern("oopAt:"),(void*[]){MKV(1L)},1L);
void* __birnam_put_args_12[]={__birnam_put_slot_12,__birnam_value_12};
(void)birnam_send(machine,__birnam_put_selector_12,__birnam_put_args_12,2L);
return birnam_send(machine,birnam_intern("callSite:"),(void*[]){MKV(1L)},1L);
}
case 2L:{
void* __birnam_put_selector_7=birnam_intern("oopAt:put:");
void* __birnam_put_slot_7=MKV(2L);
void* __birnam_value_7=birnam_global("Probe");
void* __birnam_put_args_7[]={__birnam_put_slot_7,__birnam_value_7};
(void)birnam_send(machine,__birnam_put_selector_7,__birnam_put_args_7,2L);
void* __birnam_put_selector_9=birnam_intern("oopAt:put:");
void* __birnam_put_slot_9=MKV(5L);
void* __birnam_value_9=birnam_send(machine,birnam_intern("oopAt:"),(void*[]){MKV(2L)},1L);
void* __birnam_put_args_9[]={__birnam_put_slot_9,__birnam_value_9};
(void)birnam_send(machine,__birnam_put_selector_9,__birnam_put_args_9,2L);
void* __birnam_put_selector_10=birnam_intern("oopAt:put:");
void* __birnam_put_slot_10=MKV(6L);
void* __birnam_value_10=birnam_intern("probeFinish:alias:");
void* __birnam_put_args_10[]={__birnam_put_slot_10,__birnam_value_10};
(void)birnam_send(machine,__birnam_put_selector_10,__birnam_put_args_10,2L);
void* __birnam_put_selector_11=birnam_intern("oopAt:put:");
void* __birnam_put_slot_11=MKV(7L);
void* __birnam_value_11=birnam_send(machine,birnam_intern("oopAt:"),(void*[]){MKV(0L)},1L);
void* __birnam_put_args_11[]={__birnam_put_slot_11,__birnam_value_11};
(void)birnam_send(machine,__birnam_put_selector_11,__birnam_put_args_11,2L);
void* __birnam_put_selector_12=birnam_intern("oopAt:put:");
void* __birnam_put_slot_12=MKV(8L);
void* __birnam_value_12=birnam_send(machine,birnam_intern("oopAt:"),(void*[]){MKV(1L)},1L);
void* __birnam_put_args_12[]={__birnam_put_slot_12,__birnam_value_12};
(void)birnam_send(machine,__birnam_put_selector_12,__birnam_put_args_12,2L);
return birnam_send(machine,birnam_intern("tailCallSite:"),(void*[]){MKV(2L)},1L);
}
default:return birnam_nil();
}
return birnam_nil();
}
