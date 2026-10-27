long alien_add(long a,long b){return a+b;}
typedef struct {long a,b;} Pair;
Pair alien_pair(long a){return (Pair){a,a+1};}
long alien_sum(Pair p){return p.a+p.b;}
unsigned long alien_unsigned(unsigned long a){return a;}
long alien_call(long a,long(*f)(long)){return f(a);}

typedef struct {unsigned long a;} Unsigned;
Unsigned alien_unsigned_pair(unsigned long a){return (Unsigned){a};}
double alien_double(double a){return a*2;}
