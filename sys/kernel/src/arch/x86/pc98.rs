// NEC PC-98 Series Computer Dependent

static mut PC98: Pc98 = Pc98::new();

pub struct Pc98 {
    //
}

impl Pc98 {
    const fn new() -> Self {
        Self {}
    }

    #[inline]
    fn shared<'a>() -> &'a mut Self {
        unsafe { &mut PC98 }
    }

    pub unsafe fn init() {
        //

        // mov edx,04C8Eh  ;; INT/EXT MOUSE SELECT in BIT 0
        // in al,dx
        // mov [_selMousePort],al
        // and al,0FEh
        // out dx,al
        // out 05Fh,al
        // in al,dx
        // not al
        // and al,1
        // mov [_numMousePort],al
        // mov al,[_selMousePort]
        // out dx,al

        // mov edx,0BFDBh
        // mov al,000h
        // out dx,al       ;; MOUSE = 120Hz
        // mov edx,098D7h
        // mov al,00Dh
        // out dx,al       ;; INT6, ENABLE INT
        // mov edx,07FDFh
        // mov al,093h
        // out dx,al       ;; MOUSERESET
    }
}
