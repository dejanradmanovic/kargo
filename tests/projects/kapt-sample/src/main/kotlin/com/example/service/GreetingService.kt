package com.example.service

import javax.inject.Inject

interface GreetingService {
    fun greet(name: String): String
}

class GreetingServiceImpl @Inject constructor() : GreetingService {
    override fun greet(name: String): String = "Hello, $name! (powered by Dagger 2 + KAPT)"
}
