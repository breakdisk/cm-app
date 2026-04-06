package io.logisticos.driver.core.database.di

import android.content.Context
import androidx.room.Room
import dagger.Module
import dagger.Provides
import dagger.hilt.InstallIn
import dagger.hilt.android.qualifiers.ApplicationContext
import dagger.hilt.components.SingletonComponent
import io.logisticos.driver.core.database.DriverDatabase
import javax.inject.Singleton

@Module
@InstallIn(SingletonComponent::class)
object DatabaseModule {

    @Provides @Singleton
    fun provideDatabase(@ApplicationContext context: Context): DriverDatabase =
        Room.databaseBuilder(context, DriverDatabase::class.java, "driver_app.db")
            .fallbackToDestructiveMigration()
            .build()

    @Provides fun provideShiftDao(db: DriverDatabase) = db.shiftDao()
    @Provides fun provideTaskDao(db: DriverDatabase) = db.taskDao()
    @Provides fun provideRouteDao(db: DriverDatabase) = db.routeDao()
    @Provides fun providePodDao(db: DriverDatabase) = db.podDao()
    @Provides fun provideLocationBreadcrumbDao(db: DriverDatabase) = db.locationBreadcrumbDao()
    @Provides fun provideScanEventDao(db: DriverDatabase) = db.scanEventDao()
    @Provides fun provideSyncQueueDao(db: DriverDatabase) = db.syncQueueDao()
}
