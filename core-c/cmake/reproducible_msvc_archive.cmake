function(clearra_core_enable_reproducible_msvc_archive target_name)
    if(MSVC)
        # The Rust link fingerprint hashes the external C archive. MSVC embeds
        # otherwise variable build material in objects and static libraries;
        # /Brepro keeps identical inputs byte-identical at the same path/toolchain.
        target_compile_options(${target_name} PRIVATE /Brepro)
        set_property(
            TARGET ${target_name}
            APPEND
            PROPERTY STATIC_LIBRARY_OPTIONS /Brepro
        )
    endif()
endfunction()
