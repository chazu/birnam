/* Receiver-first calls exercise the real Alien marshaller and callback entry. */
long oop_callback(long (*callback)(long), long value) { return callback(value); }
unsigned long oop_unsigned(unsigned long value) { return value; }
const char *oop_string(const char *value) { return value; }
double oop_double(double value) { return value; }
