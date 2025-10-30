#ifndef __LIBID_ENHANCED_H
#define __LIBID_ENHANCED_H

/* Non-variadic debug printf macros */
#if DEBUG_ALL
# define dprintf0(fmt)                    fprintf(stderr, fmt)
# define dprintf1(fmt, a1)                fprintf(stderr, fmt, a1)
# define dprintf2(fmt, a1, a2)            fprintf(stderr, fmt, a1, a2)
# define dprintf3(fmt, a1, a2, a3)        fprintf(stderr, fmt, a1, a2, a3)
# define dprintf4(fmt, a1, a2, a3, a4)    fprintf(stderr, fmt, a1, a2, a3, a4)
# define dprintf5(fmt, a1, a2, a3, a4, a5) fprintf(stderr, fmt, a1, a2, a3, a4, a5)
# define dprintf6(fmt, a1, a2, a3, a4, a5, a6) fprintf(stderr, fmt, a1, a2, a3, a4, a5, a6)
# define dprintf7(fmt, a1, a2, a3, a4, a5, a6, a7) fprintf(stderr, fmt, a1, a2, a3, a4, a5, a6, a7)
#else
# define dprintf0(fmt)
# define dprintf1(fmt, a1)
# define dprintf2(fmt, a1, a2)
# define dprintf3(fmt, a1, a2, a3)
# define dprintf4(fmt, a1, a2, a3, a4)
# define dprintf5(fmt, a1, a2, a3, a4, a5)
# define dprintf6(fmt, a1, a2, a3, a4, a5, a6)
# define dprintf7(fmt, a1, a2, a3, a4, a5, a6, a7)
#endif

/* Non-variadic method dispatch macros - compatible with generated code */
#define _sendv0(MSG, RCV) ({						\
  struct __send _s= { (MSG), 1, (RCV), NULL, NULL };			\
  ((_imp_t)(_libid->bindv(&_s)))(&_s, _s.receiver, _s.receiver);		\
})

#define _sendv1(MSG, RCV, A1) ({					\
  struct __send _s= { (MSG), 2, (RCV), NULL, NULL };			\
  ((_imp_t)(_libid->bindv(&_s)))(&_s, _s.receiver, _s.receiver, A1);	\
})

#define _sendv2(MSG, RCV, A1, A2) ({					\
  struct __send _s= { (MSG), 3, (RCV), NULL, NULL };			\
  ((_imp_t)(_libid->bindv(&_s)))(&_s, _s.receiver, _s.receiver, A1, A2);	\
})

#define _sendv3(MSG, RCV, A1, A2, A3) ({				\
  struct __send _s= { (MSG), 4, (RCV), NULL, NULL };			\
  ((_imp_t)(_libid->bindv(&_s)))(&_s, _s.receiver, _s.receiver, A1, A2, A3); \
})

#define _sendv4(MSG, RCV, A1, A2, A3, A4) ({				\
  struct __send _s= { (MSG), 5, (RCV), NULL, NULL };			\
  ((_imp_t)(_libid->bindv(&_s)))(&_s, _s.receiver, _s.receiver, A1, A2, A3, A4); \
})

#define _sendv5(MSG, RCV, A1, A2, A3, A4, A5) ({			\
  struct __send _s= { (MSG), 6, (RCV), NULL, NULL };			\
  ((_imp_t)(_libid->bindv(&_s)))(&_s, _s.receiver, _s.receiver, A1, A2, A3, A4, A5); \
})

/* Non-variadic super dispatch macros - compatible with generated code */
#define _superv0(TYP, MSG, RCV) ({					\
  struct __send _s= { (MSG), 1, (TYP) };				\
  _imp_t _imp= _libid->bindv(&_s);					\
  _s.receiver= (RCV);							\
  _imp(&_s, _s.receiver, _s.receiver);					\
})

#define _superv1(TYP, MSG, RCV, A1) ({					\
  struct __send _s= { (MSG), 2, (TYP) };				\
  _imp_t _imp= _libid->bindv(&_s);					\
  _s.receiver= (RCV);							\
  _imp(&_s, _s.receiver, _s.receiver, A1);				\
})

#define _superv2(TYP, MSG, RCV, A1, A2) ({				\
  struct __send _s= { (MSG), 3, (TYP) };				\
  _imp_t _imp= _libid->bindv(&_s);					\
  _s.receiver= (RCV);							\
  _imp(&_s, _s.receiver, _s.receiver, A1, A2);				\
})

#define _superv3(TYP, MSG, RCV, A1, A2, A3) ({				\
  struct __send _s= { (MSG), 4, (TYP) };				\
  _imp_t _imp= _libid->bindv(&_s);					\
  _s.receiver= (RCV);							\
  _imp(&_s, _s.receiver, _s.receiver, A1, A2, A3);			\
})

#define _superv4(TYP, MSG, RCV, A1, A2, A3, A4) ({			\
  struct __send _s= { (MSG), 5, (TYP) };				\
  _imp_t _imp= _libid->bindv(&_s);					\
  _s.receiver= (RCV);							\
  _imp(&_s, _s.receiver, _s.receiver, A1, A2, A3, A4);			\
})

/* Forward declaration for _libid - will be defined in libid.c */

/* Managed code macros (for CManagedCodeGenerator) - compatible with generated code */
#define _send0(MSG, RCV) ({						\
    register oop _r= (RCV);						\
    struct __closure *_c= (struct __closure *)_libid->bind((MSG), _r);	\
    (_c->method)((oop)_c, _r, _r);					\
})

#define _send1(MSG, RCV, A1) ({						\
    register oop _r= (RCV);						\
    struct __closure *_c= (struct __closure *)_libid->bind((MSG), _r);	\
    (_c->method)((oop)_c, _r, _r, A1);					\
})

#define _send2(MSG, RCV, A1, A2) ({					\
    register oop _r= (RCV);						\
    struct __closure *_c= (struct __closure *)_libid->bind((MSG), _r);	\
    (_c->method)((oop)_c, _r, _r, A1, A2);				\
})

#define _send3(MSG, RCV, A1, A2, A3) ({					\
    register oop _r= (RCV);						\
    struct __closure *_c= (struct __closure *)_libid->bind((MSG), _r);	\
    (_c->method)((oop)_c, _r, _r, A1, A2, A3);				\
})

#define _send4(MSG, RCV, A1, A2, A3, A4) ({				\
    register oop _r= (RCV);						\
    struct __closure *_c= (struct __closure *)_libid->bind((MSG), _r);	\
    (_c->method)((oop)_c, _r, _r, A1, A2, A3, A4);			\
})

#define _super0(TYP, MSG, RCV) ({					\
  register oop _r= (RCV);						\
  struct __closure *_c= (struct __closure *)_libid->bind((MSG), (TYP));	\
  (_c->method)((oop)_c, _r, _r);					\
})

#define _super1(TYP, MSG, RCV, A1) ({					\
  register oop _r= (RCV);						\
  struct __closure *_c= (struct __closure *)_libid->bind((MSG), (TYP));	\
  (_c->method)((oop)_c, _r, _r, A1);					\
})

#define _super2(TYP, MSG, RCV, A1, A2) ({				\
  register oop _r= (RCV);						\
  struct __closure *_c= (struct __closure *)_libid->bind((MSG), (TYP));	\
  (_c->method)((oop)_c, _r, _r, A1, A2);				\
})

#define _super3(TYP, MSG, RCV, A1, A2, A3) ({				\
  register oop _r= (RCV);						\
  struct __closure *_c= (struct __closure *)_libid->bind((MSG), (TYP));	\
  (_c->method)((oop)_c, _r, _r, A1, A2, A3);				\
})

#define _super4(TYP, MSG, RCV, A1, A2, A3, A4) ({			\
  register oop _r= (RCV);						\
  struct __closure *_c= (struct __closure *)_libid->bind((MSG), (TYP));	\
  (_c->method)((oop)_c, _r, _r, A1, A2, A3, A4);			\
})

/* Dynamic dispatch function declarations */
oop _libid_sendv_dispatch(oop selector, oop receiver, int argc, oop *argv);
oop _libid_superv_dispatch(oop type, oop selector, oop receiver, int argc, oop *argv);
oop _libid_send_dispatch(oop selector, oop receiver, int argc, oop *argv);
oop _libid_super_dispatch(oop type, oop selector, oop receiver, int argc, oop *argv);

#endif /* __LIBID_ENHANCED_H */
